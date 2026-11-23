// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity 0.8.28;

import {Test} from "forge-std/Test.sol";
import {BountyBoard} from "../src/BountyBoard.sol";
import {CoveringRegistry} from "../src/CoveringRegistry.sol";
import {FalseToken, Ledger, PlainToken, SilentToken} from "./Tokens.sol";

/// Burns `feeBps` hundredths of a percent of every amount it moves, as fee-on-transfer tokens do.
contract FeeToken is PlainToken {
    uint256 immutable feeBps;

    constructor(uint256 feeBps_) {
        feeBps = feeBps_;
    }

    function _send(address from, address to, uint256 amount) internal override {
        balanceOf[from] -= amount;
        balanceOf[to] += amount - amount * feeBps / 10_000;
    }
}

/// Makes one call it was given at the start of its next move, and keeps what came back.
contract ReentrantToken is PlainToken {
    address target;
    bytes data;
    bool public ok;
    bytes public answer;

    function arm(address target_, bytes calldata data_) external {
        target = target_;
        data = data_;
    }

    function _send(address from, address to, uint256 amount) internal override {
        address called = target;
        if (called != address(0)) {
            delete target;
            (ok, answer) = called.call(data);
        }
        super._send(from, to, amount);
    }
}

/// Makes several calls in one transaction, as a smart wallet or a multicall contract does.
contract Batch {
    function run(address[] calldata targets, bytes[] calldata calls) external {
        for (uint256 i; i < calls.length; ++i) {
            (bool ok, bytes memory answer) = targets[i].call(calls[i]);
            if (!ok) {
                assembly ("memory-safe") {
                    revert(add(answer, 32), mload(answer))
                }
            }
        }
    }
}

contract BountyBoardTest is Test {
    // The Fano plane, C(7, 3, 2) = 7, one block per hex string. It meets the Schoenheim bound for (7, 3).
    bytes constant FANO = hex"000102" hex"000304" hex"000506" hex"010305" hex"010406" hex"020306" hex"020405";
    // C(5, 3, 2) = 4, which meets the Schoenheim bound for (5, 3).
    bytes constant FIVE = hex"000102" hex"000304" hex"010304" hex"020304";
    uint256 constant START = 1e30;

    CoveringRegistry registry;
    PlainToken token;
    BountyBoard board;
    address funder = makeAddr("funder");
    address other = makeAddr("other funder");
    address finder = makeAddr("finder");
    address later = makeAddr("later finder");
    address stranger = makeAddr("stranger");
    address[3] funders = [makeAddr("funder 0"), makeAddr("funder 1"), makeAddr("funder 2")];
    address[3] finders = [makeAddr("finder 0"), makeAddr("finder 1"), makeAddr("finder 2")];

    /// What the board should hold after a sequence of calls, kept with plain arithmetic.
    struct Model {
        BountyBoard board;
        FeeToken token;
        uint256 feeBps;
        uint32[2] records; // per entry, 0 when there is none
        uint256[2] holders; // per entry, the finder of the record
        uint256[5][2] pots; // per entry and target, the target being the bound minus 1 plus the index
        uint256[3] spent; // per funder
        uint256[3] earned; // per finder
    }

    function setUp() public {
        registry = new CoveringRegistry();
        token = new PlainToken();
        board = _boardFor(token);
    }

    function test_ConstructorKeepsItsArguments() public view {
        assertEq(address(board.registry()), address(registry));
        assertEq(board.token(), address(token));
    }

    function test_ConstructorRejectsAccountsWithoutCode() public {
        address empty = makeAddr("no code");
        vm.expectRevert(abi.encodeWithSelector(BountyBoard.NotAContract.selector, address(0)));
        new BountyBoard(registry, address(0));
        vm.expectRevert(abi.encodeWithSelector(BountyBoard.NotAContract.selector, empty));
        new BountyBoard(registry, empty);
        vm.expectRevert(abi.encodeWithSelector(BountyBoard.NotAContract.selector, address(0)));
        new BountyBoard(CoveringRegistry(address(0)), address(token));
        vm.expectRevert(abi.encodeWithSelector(BountyBoard.NotAContract.selector, empty));
        new BountyBoard(CoveringRegistry(empty), address(token));
    }

    function test_FundAddsToThePot() public {
        vm.expectEmit(address(board));
        emit BountyBoard.Funded(funder, 83, 17, 30, 100, 100);
        _fund(funder, 83, 17, 30, 100);
        vm.expectEmit(address(board));
        emit BountyBoard.Funded(other, 83, 17, 30, 50, 150);
        _fund(other, 83, 17, 30, 50);

        assertEq(board.potOf(83, 17, 30), 150);
        assertEq(token.balanceOf(address(board)), 150);
        assertEq(token.balanceOf(funder), START - 100);
        assertEq(token.balanceOf(other), START - 50);
        // Every target is a pot of its own.
        assertEq(board.potOf(83, 17, 31), 0);
        assertEq(board.potOf(83, 16, 30), 0);
        assertEq(board.potOf(84, 17, 30), 0);
    }

    function test_FundRejectsInvalidParameters() public {
        _expectFundError(2, 2, 100, 1, abi.encodeWithSelector(BountyBoard.InvalidParameters.selector, 2, 2));
        _expectFundError(256, 3, 100, 1, abi.encodeWithSelector(BountyBoard.InvalidParameters.selector, 256, 3));
        _expectFundError(7, 0, 100, 1, abi.encodeWithSelector(BountyBoard.InvalidParameters.selector, 7, 0));
        _expectFundError(7, 1, 100, 1, abi.encodeWithSelector(BountyBoard.InvalidParameters.selector, 7, 1));
        _expectFundError(7, 7, 100, 1, abi.encodeWithSelector(BountyBoard.InvalidParameters.selector, 7, 7));
        _expectFundError(7, 8, 100, 1, abi.encodeWithSelector(BountyBoard.InvalidParameters.selector, 7, 8));
        // The ends of the range are taken.
        _fund(funder, 3, 2, 3, 1);
        _fund(funder, 255, 254, 3, 1);
        assertEq(token.balanceOf(address(board)), 2);
    }

    function test_FundRejectsZeroAmount() public {
        _expectFundError(7, 3, 7, 0, abi.encodeWithSelector(BountyBoard.ZeroAmount.selector));
    }

    function test_FundTakesTargetsDownToTheBoundExactly() public {
        _expectBound(7, 3, 7); // the Fano plane meets it
        _expectBound(9, 3, 12); // the affine plane of order 3
        _expectBound(13, 4, 13); // the projective plane of order 3
        _expectBound(83, 17, 30); // record 001
        _expectBound(3, 2, 3);
        _expectBound(255, 2, 32_385);
        _expectBound(255, 254, 3);
    }

    /// The bound on chain agrees with the same bound computed by testing remainders.
    function testFuzz_BoundMatchesReference(uint16 v, uint8 k) public {
        v = uint16(bound(v, 3, 255));
        k = uint8(bound(k, 2, v - 1));
        uint256 perPoint = (uint256(v) - 1) / (k - 1);
        if (perPoint * (k - 1) < v - 1) ++perPoint;
        uint256 blocks = v * perPoint / k;
        if (blocks * k < v * perPoint) ++blocks;
        _expectBound(v, k, uint32(blocks));
    }

    function test_FundRejectsATargetTheRecordMeets() public {
        _submit(finder, 7, 3, _covering(FANO, 7, 8));
        _expectFundError(7, 3, 8, 1, abi.encodeWithSelector(BountyBoard.TargetAlreadyMet.selector, 8, 8));
        _expectFundError(7, 3, 9, 1, abi.encodeWithSelector(BountyBoard.TargetAlreadyMet.selector, 9, 8));
        _fund(funder, 7, 3, 7, 1);
        assertEq(board.potOf(7, 3, 7), 1);
    }

    function test_ClaimPaysTheFinderOfARecordThatArrivedLater() public {
        _fund(funder, 7, 3, 7, 100);
        bytes32 id = _submit(finder, 7, 3, FANO);
        vm.expectEmit(address(board));
        emit BountyBoard.Paid(finder, 7, 3, 7, 100, id);
        vm.prank(stranger);
        board.claim(7, 3, 7);

        assertEq(token.balanceOf(finder), 100);
        assertEq(token.balanceOf(stranger), 0);
        assertEq(token.balanceOf(address(board)), 0);
        assertEq(board.potOf(7, 3, 7), 0);
    }

    function test_ClaimPaysARecordThatMeetsTheTargetExactly() public {
        _fund(funder, 7, 3, 8, 100);
        _submit(finder, 7, 3, _covering(FANO, 7, 8));
        board.claim(7, 3, 8);
        assertEq(token.balanceOf(finder), 100);
    }

    function test_ClaimRejectsAnUnmetTarget() public {
        _fund(funder, 7, 3, 7, 100);
        vm.expectRevert(abi.encodeWithSelector(BountyBoard.TargetNotMet.selector, 7, 0));
        board.claim(7, 3, 7);
        _submit(finder, 7, 3, _covering(FANO, 7, 8));
        vm.expectRevert(abi.encodeWithSelector(BountyBoard.TargetNotMet.selector, 7, 8));
        board.claim(7, 3, 7);
        assertEq(board.potOf(7, 3, 7), 100);
    }

    function test_ClaimRejectsAnEmptyPot() public {
        _submit(finder, 7, 3, FANO);
        vm.expectRevert(BountyBoard.NoPot.selector);
        board.claim(7, 3, 7);
        vm.expectRevert(BountyBoard.NoPot.selector);
        board.claim(7, 7, 7);
    }

    function test_ClaimPaysOnce() public {
        _fund(funder, 7, 3, 7, 100);
        _submit(finder, 7, 3, FANO);
        board.claim(7, 3, 7);
        vm.expectRevert(BountyBoard.NoPot.selector);
        board.claim(7, 3, 7);
        assertEq(token.balanceOf(finder), 100);
    }

    function test_SmallerTargetOnTheSameEntryWaitsForItsOwnRecord() public {
        _fund(funder, 7, 3, 9, 100);
        _fund(other, 7, 3, 7, 50);
        _submit(finder, 7, 3, _covering(FANO, 7, 8));
        board.claim(7, 3, 9);
        vm.expectRevert(abi.encodeWithSelector(BountyBoard.TargetNotMet.selector, 7, 8));
        board.claim(7, 3, 7);

        _submit(later, 7, 3, FANO);
        board.claim(7, 3, 7);
        assertEq(token.balanceOf(finder), 100);
        assertEq(token.balanceOf(later), 50);
        assertEq(token.balanceOf(address(board)), 0);
    }

    function test_PaysWhoeverHoldsTheRecordWhenClaimed() public {
        _fund(funder, 7, 3, 8, 100);
        _submit(finder, 7, 3, _covering(FANO, 7, 8));
        bytes32 id = _submit(later, 7, 3, FANO);
        vm.expectEmit(address(board));
        emit BountyBoard.Paid(later, 7, 3, 8, 100, id);
        board.claim(7, 3, 8);
        assertEq(token.balanceOf(finder), 0);
        assertEq(token.balanceOf(later), 100);
    }

    function test_PotHoldsWhatArrivesFromAFeeToken() public {
        FeeToken fee = new FeeToken(100);
        BountyBoard feeBoard = _boardFor(fee);
        vm.expectEmit(address(feeBoard));
        emit BountyBoard.Funded(funder, 7, 3, 7, 990, 990);
        vm.prank(funder);
        feeBoard.fund(7, 3, 7, 1000);
        assertEq(feeBoard.potOf(7, 3, 7), 990);
        assertEq(fee.balanceOf(address(feeBoard)), 990);

        bytes32 id = _submit(finder, 7, 3, FANO);
        vm.expectEmit(address(feeBoard));
        emit BountyBoard.Paid(finder, 7, 3, 7, 990, id);
        feeBoard.claim(7, 3, 7);
        assertEq(fee.balanceOf(finder), 990 - 9);
        assertEq(fee.balanceOf(address(feeBoard)), 0);
    }

    function test_AcceptsTokenThatReturnsNothing() public {
        SilentToken silent = new SilentToken();
        BountyBoard silentBoard = _boardFor(silent);
        vm.prank(funder);
        silentBoard.fund(7, 3, 7, 100);
        _submit(finder, 7, 3, FANO);
        silentBoard.claim(7, 3, 7);
        assertEq(silent.balanceOf(finder), 100);
        assertEq(silent.balanceOf(address(silentBoard)), 0);
    }

    function test_RejectsTokenThatReturnsFalse() public {
        BountyBoard falseBoard = _boardFor(new FalseToken());
        vm.expectRevert(BountyBoard.TransferFailed.selector);
        vm.prank(funder);
        falseBoard.fund(7, 3, 7, 100);
        assertEq(falseBoard.potOf(7, 3, 7), 0);
    }

    function test_RejectsMissingApprovalOrBalance() public {
        SilentToken silent = new SilentToken();
        BountyBoard silentBoard = _boardFor(silent);
        vm.startPrank(funder);
        token.approve(address(board), 99);
        silent.approve(address(silentBoard), 99);
        vm.expectRevert(BountyBoard.TransferFailed.selector);
        board.fund(7, 3, 7, 100);
        vm.expectRevert(BountyBoard.TransferFailed.selector);
        silentBoard.fund(7, 3, 7, 100);
        vm.stopPrank();

        vm.startPrank(stranger);
        token.approve(address(board), 100);
        vm.expectRevert(BountyBoard.TransferFailed.selector);
        board.fund(7, 3, 7, 100);
        vm.stopPrank();
        assertEq(token.balanceOf(address(board)) + silent.balanceOf(address(silentBoard)), 0);
    }

    function test_FailedPayoutKeepsThePot() public {
        _fund(funder, 7, 3, 7, 100);
        _submit(finder, 7, 3, FANO);
        vm.mockCallRevert(address(token), abi.encodeWithSelector(PlainToken.transfer.selector), "");
        vm.expectRevert(BountyBoard.TransferFailed.selector);
        board.claim(7, 3, 7);
        vm.mockCall(address(token), abi.encodeWithSelector(PlainToken.transfer.selector), abi.encode(false));
        vm.expectRevert(BountyBoard.TransferFailed.selector);
        board.claim(7, 3, 7);
        vm.clearMockedCalls();

        assertEq(board.potOf(7, 3, 7), 100);
        board.claim(7, 3, 7);
        assertEq(token.balanceOf(finder), 100);
    }

    function test_FundCannotBeReentered() public {
        ReentrantToken evil = new ReentrantToken();
        BountyBoard evilBoard = _boardFor(evil);
        // The token holds tokens and has approved the board, so a nested fund would go through if it were let in,
        // and the outer fund would then count the nested deposit as its own.
        evil.mint(address(evil), 50);
        vm.prank(address(evil));
        evil.approve(address(evilBoard), 50);
        evil.arm(address(evilBoard), abi.encodeCall(BountyBoard.fund, (7, 3, 8, 50)));
        vm.prank(funder);
        evilBoard.fund(7, 3, 7, 100);

        assertFalse(evil.ok());
        assertEq(evil.answer(), abi.encodeWithSelector(BountyBoard.Reentered.selector));
        assertEq(evilBoard.potOf(7, 3, 7), 100);
        assertEq(evilBoard.potOf(7, 3, 8), 0);
        assertEq(evil.balanceOf(address(evilBoard)), 100);
    }

    function test_ClaimCannotBeReentered() public {
        ReentrantToken evil = new ReentrantToken();
        BountyBoard evilBoard = _boardFor(evil);
        vm.startPrank(funder);
        evilBoard.fund(7, 3, 7, 100);
        evilBoard.fund(7, 3, 8, 100);
        vm.stopPrank();
        _submit(finder, 7, 3, FANO);
        evil.arm(address(evilBoard), abi.encodeCall(BountyBoard.claim, (7, 3, 7)));
        evilBoard.claim(7, 3, 7);

        assertFalse(evil.ok());
        assertEq(evil.answer(), abi.encodeWithSelector(BountyBoard.Reentered.selector));
        assertEq(evil.balanceOf(finder), 100);
        assertEq(evilBoard.potOf(7, 3, 8), 100);
        assertEq(evil.balanceOf(address(evilBoard)), 100);
    }

    function test_ClaimEmptiesThePotBeforePaying() public {
        ReentrantToken evil = new ReentrantToken();
        BountyBoard evilBoard = _boardFor(evil);
        vm.prank(funder);
        evilBoard.fund(7, 3, 7, 100);
        _submit(finder, 7, 3, FANO);
        evil.arm(address(evilBoard), abi.encodeCall(BountyBoard.potOf, (7, 3, 7)));
        evilBoard.claim(7, 3, 7);

        assertTrue(evil.ok());
        assertEq(evil.answer(), abi.encode(uint256(0)));
    }

    /// A contract can fund, submit a record, claim and fund again in one transaction: the lock is released after
    /// every call, not only at the end of the transaction.
    function test_OneTransactionCanFundAndClaimInTurn() public {
        Batch batch = new Batch();
        token.mint(address(batch), 200);
        address[] memory targets = new address[](5);
        bytes[] memory calls = new bytes[](5);
        (targets[0], calls[0]) = (address(token), abi.encodeCall(Ledger.approve, (address(board), 200)));
        (targets[1], calls[1]) = (address(board), abi.encodeCall(BountyBoard.fund, (7, 3, 7, 100)));
        (targets[2], calls[2]) = (address(registry), abi.encodeCall(CoveringRegistry.submit, (7, 3, FANO, address(0))));
        (targets[3], calls[3]) = (address(board), abi.encodeCall(BountyBoard.claim, (7, 3, 7)));
        (targets[4], calls[4]) = (address(board), abi.encodeCall(BountyBoard.fund, (83, 17, 30, 100)));
        batch.run(targets, calls);

        assertEq(token.balanceOf(address(batch)), 100);
        assertEq(board.potOf(7, 3, 7), 0);
        assertEq(board.potOf(83, 17, 30), 100);
    }

    /// Random sequences of funds, records and claims on two entries, with a token that may take a fee: every call
    /// must succeed or fail as the model says, and every pot and balance must match it after every step.
    function testFuzz_FundAndClaimMatchModel(uint256 seed, uint16 feeBps) public {
        Model memory model;
        model.feeBps = bound(feeBps, 0, 2000);
        model.token = new FeeToken(model.feeBps);
        model.board = _boardFor(model.token);
        for (uint256 step; step < 24; ++step) {
            uint256 r = uint256(keccak256(abi.encode(seed, step)));
            if (r % 5 < 2) _fundStep(model, r);
            else if (r % 5 == 2) _recordStep(model, r);
            else _claimStep(model, r);
            _compare(model);
        }
    }

    function _fundStep(Model memory model, uint256 r) internal {
        uint256 entry = (r >> 8) % 2;
        (uint16 v, uint32 lowest) = _entry(entry);
        uint32 target = lowest - 1 + uint32((r >> 16) % 5);
        uint256 amount = (r >> 24) % 8 == 0 ? 0 : 1 + (r >> 32) % 1e24;
        uint256 who = (r >> 128) % 3;
        uint32 record = model.records[entry];
        bytes memory expected;
        if (amount == 0) {
            expected = abi.encodeWithSelector(BountyBoard.ZeroAmount.selector);
        } else if (target < lowest) {
            expected = abi.encodeWithSelector(BountyBoard.ImpossibleTarget.selector, target, lowest);
        } else if (record != 0 && record <= target) {
            expected = abi.encodeWithSelector(BountyBoard.TargetAlreadyMet.selector, target, record);
        }
        vm.prank(funders[who]);
        (bool ok, bytes memory answer) =
            address(model.board).call(abi.encodeCall(BountyBoard.fund, (v, 3, target, amount)));
        assertEq(answer, expected);
        assertEq(ok, expected.length == 0);
        if (ok) {
            model.pots[entry][target + 1 - lowest] += amount - amount * model.feeBps / 10_000;
            model.spent[who] += amount;
        }
    }

    function _recordStep(Model memory model, uint256 r) internal {
        uint256 entry = (r >> 8) % 2;
        (uint16 v, uint32 lowest) = _entry(entry);
        uint32 size = lowest + uint32((r >> 16) % 4);
        uint256 who = (r >> 128) % 3;
        bytes memory blocks = _covering(entry == 0 ? FANO : FIVE, v, size);
        vm.prank(finders[who]);
        (bool ok,) = address(registry).call(abi.encodeCall(CoveringRegistry.submit, (v, 3, blocks, address(0))));
        assertEq(ok, model.records[entry] == 0 || size < model.records[entry]);
        if (ok) (model.records[entry], model.holders[entry]) = (size, who);
    }

    function _claimStep(Model memory model, uint256 r) internal {
        uint256 entry = (r >> 8) % 2;
        (uint16 v, uint32 lowest) = _entry(entry);
        // Three times in four, the first target from a random one on that holds a pot, if any does.
        uint256 slot = (r >> 16) % 5;
        for (uint256 i; i < 5 && (r >> 40) % 4 != 0; ++i) {
            if (model.pots[entry][(slot + i) % 5] != 0) {
                slot = (slot + i) % 5;
                break;
            }
        }
        uint32 target = lowest - 1 + uint32(slot);
        uint256 pot = model.pots[entry][slot];
        uint32 record = model.records[entry];
        bytes memory expected;
        if (pot == 0) {
            expected = abi.encodeWithSelector(BountyBoard.NoPot.selector);
        } else if (record == 0 || record > target) {
            expected = abi.encodeWithSelector(BountyBoard.TargetNotMet.selector, target, record);
        }
        vm.prank(stranger);
        (bool ok, bytes memory answer) = address(model.board).call(abi.encodeCall(BountyBoard.claim, (v, 3, target)));
        assertEq(answer, expected);
        assertEq(ok, expected.length == 0);
        if (ok) {
            model.pots[entry][slot] = 0;
            model.earned[model.holders[entry]] += pot - pot * model.feeBps / 10_000;
        }
    }

    /// Every pot, the board's balance and every funder's and finder's balance, against the model.
    function _compare(Model memory model) internal view {
        uint256 total;
        for (uint256 entry; entry < 2; ++entry) {
            (uint16 v, uint32 lowest) = _entry(entry);
            for (uint256 i; i < 5; ++i) {
                assertEq(model.board.potOf(v, 3, lowest - 1 + uint32(i)), model.pots[entry][i]);
                total += model.pots[entry][i];
            }
        }
        assertEq(model.token.balanceOf(address(model.board)), total);
        for (uint256 who; who < 3; ++who) {
            assertEq(model.token.balanceOf(funders[who]), START - model.spent[who]);
            assertEq(model.token.balanceOf(finders[who]), model.earned[who]);
        }
    }

    /// The model's two entries, (7, 3) and (5, 3), with their Schoenheim bounds.
    function _entry(uint256 entry) internal pure returns (uint16 v, uint32 lowest) {
        return entry == 0 ? (uint16(7), uint32(7)) : (uint16(5), uint32(4));
    }

    /// A board for `asset`, with START tokens minted to each funder, all of them approved for the board.
    function _boardFor(Ledger asset) internal returns (BountyBoard created) {
        created = new BountyBoard(registry, address(asset));
        address[5] memory holders = [funder, other, funders[0], funders[1], funders[2]];
        for (uint256 i; i < holders.length; ++i) {
            asset.mint(holders[i], START);
            vm.prank(holders[i]);
            asset.approve(address(created), type(uint256).max);
        }
    }

    function _fund(address who, uint16 v, uint8 k, uint32 maxBlocks, uint256 amount) internal {
        vm.prank(who);
        board.fund(v, k, maxBlocks, amount);
    }

    function _submit(address who, uint16 v, uint8 k, bytes memory blocks) internal returns (bytes32) {
        vm.prank(who);
        return registry.submit(v, k, blocks, address(0));
    }

    function _expectFundError(uint16 v, uint8 k, uint32 maxBlocks, uint256 amount, bytes memory error_) internal {
        vm.expectRevert(error_);
        _fund(funder, v, k, maxBlocks, amount);
    }

    /// Funding one block below `lowest` must fail with the bound named, and funding at `lowest` must work.
    function _expectBound(uint16 v, uint8 k, uint32 lowest) internal {
        _expectFundError(
            v, k, lowest - 1, 1, abi.encodeWithSelector(BountyBoard.ImpossibleTarget.selector, lowest - 1, lowest)
        );
        _fund(funder, v, k, lowest, 1);
        assertEq(board.potOf(v, k, lowest), 1);
    }

    /// A canonical (v, 3, 2) covering of `size` blocks: the covering `base`, which is in canonical order, with the
    /// first triples it lacks added in lexicographic order.
    function _covering(bytes memory base, uint256 v, uint256 size) internal pure returns (bytes memory blocks) {
        uint256 extra = size - base.length / 3;
        for (uint256 a; a < v; ++a) {
            for (uint256 b = a + 1; b < v; ++b) {
                for (uint256 c = b + 1; c < v; ++c) {
                    bool inBase;
                    for (uint256 i; i < base.length; i += 3) {
                        inBase = inBase || (uint8(base[i]) == a && uint8(base[i + 1]) == b && uint8(base[i + 2]) == c);
                    }
                    if (!inBase && extra == 0) continue;
                    if (!inBase) --extra;
                    blocks = bytes.concat(blocks, abi.encodePacked(uint8(a), uint8(b), uint8(c)));
                }
            }
        }
    }
}
