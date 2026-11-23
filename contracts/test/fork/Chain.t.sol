// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity 0.8.28;

import {Test} from "forge-std/Test.sol";
import {IERC20} from "forge-std/interfaces/IERC20.sol";
import {RECORD_001} from "../../script/Record001.sol";
import {BountyBoard} from "../../src/BountyBoard.sol";
import {CoveringRegistry} from "../../src/CoveringRegistry.sol";
import {HuntSponsor} from "../../src/HuntSponsor.sol";

/// The contracts with real tokens, on a fork of Robinhood Chain mainnet. RH_RPC_URL names an RPC endpoint of the
/// chain, ZETEMA_FORK_TOKEN_GRAD a token that has left its launch curve and ZETEMA_FORK_TOKEN_CURVE one still on it.
/// Without them the tests skip themselves, so CI never reaches the network.
contract ChainForkTest is Test {
    uint256 constant CHAIN_ID = 4663;
    address constant BURN = 0x000000000000000000000000000000000000dEaD;
    uint256 constant UNIT = 1000e18;

    address user = makeAddr("user");
    address finder = makeAddr("finder");

    function test_SponsorBurnsAGraduatedToken() public {
        _sponsorBurns(_fork("ZETEMA_FORK_TOKEN_GRAD"));
    }

    function test_SponsorBurnsACurveToken() public {
        _sponsorBurns(_fork("ZETEMA_FORK_TOKEN_CURVE"));
    }

    function test_BountyPaysInAGraduatedToken() public {
        _bountyPays(_fork("ZETEMA_FORK_TOKEN_GRAD"));
    }

    function test_BountyPaysInACurveToken() public {
        _bountyPays(_fork("ZETEMA_FORK_TOKEN_CURVE"));
    }

    /// Forks the chain at its latest block and returns the token named in `variable`; skips the test when the RPC
    /// endpoint or the token is not set.
    function _fork(string memory variable) internal returns (IERC20 token) {
        string memory rpc = vm.envOr("RH_RPC_URL", string(""));
        token = IERC20(vm.envOr(variable, address(0)));
        if (bytes(rpc).length == 0 || address(token) == address(0)) vm.skip(true);
        vm.createSelectFork(rpc);
        assertEq(block.chainid, CHAIN_ID);
        assertGt(address(token).code.length, 0);
    }

    /// Three hours of search burn three units, and the tokens land on the burn address.
    function _sponsorBurns(IERC20 token) internal {
        HuntSponsor hunt = new HuntSponsor(address(token), UNIT, 168);
        deal(address(token), user, 10 * UNIT);
        uint256 burned = token.balanceOf(BURN);

        vm.startPrank(user);
        token.approve(address(hunt), 3 * UNIT);
        vm.expectEmit(address(hunt));
        emit HuntSponsor.Sponsored(user, 83, 17, 2, 3 * UNIT, 3);
        hunt.sponsor(83, 17, 2, 3);
        vm.stopPrank();

        assertEq(token.balanceOf(BURN), burned + 3 * UNIT);
        assertEq(token.balanceOf(user), 7 * UNIT);
        assertEq(token.balanceOf(address(hunt)), 0);
    }

    /// A pot on C(83, 17, 2) <= 30 goes into the board, record 001 is submitted, and a claim by someone else pays the
    /// finder.
    function _bountyPays(IERC20 token) internal {
        CoveringRegistry registry = new CoveringRegistry();
        BountyBoard board = new BountyBoard(registry, address(token));
        deal(address(token), user, 10 * UNIT);

        vm.startPrank(user);
        token.approve(address(board), 4 * UNIT);
        board.fund(83, 17, 30, 4 * UNIT);
        vm.stopPrank();
        assertEq(board.potOf(83, 17, 30), 4 * UNIT);
        assertEq(token.balanceOf(address(board)), 4 * UNIT);

        vm.prank(finder);
        bytes32 id = registry.submit(83, 17, RECORD_001, user);
        vm.expectEmit(address(board));
        emit BountyBoard.Paid(finder, 83, 17, 30, 4 * UNIT, id);
        board.claim(83, 17, 30);

        assertEq(token.balanceOf(finder), 4 * UNIT);
        assertEq(token.balanceOf(address(board)), 0);
        assertEq(token.balanceOf(user), 6 * UNIT);
        assertEq(board.potOf(83, 17, 30), 0);
    }
}
