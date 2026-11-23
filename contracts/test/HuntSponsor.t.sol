// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity 0.8.28;

import {Test} from "forge-std/Test.sol";
import {HuntSponsor} from "../src/HuntSponsor.sol";
import {FalseToken, Ledger, PlainToken, SilentToken} from "./Tokens.sol";

contract HuntSponsorTest is Test {
    address constant DEAD = 0x000000000000000000000000000000000000dEaD;
    uint256 constant UNIT = 250e18;
    uint256 constant MAX_HOURS = 168;

    address user = makeAddr("user");

    function test_BurnsHoursTimesUnit() public {
        PlainToken token = new PlainToken();
        HuntSponsor hunt = _funded(token, 24 * UNIT);
        vm.expectEmit(address(hunt));
        emit HuntSponsor.Sponsored(user, 83, 17, 2, 24 * UNIT, 24);
        vm.prank(user);
        hunt.sponsor(83, 17, 2, 24);

        assertEq(token.balanceOf(DEAD), 24 * UNIT);
        assertEq(token.balanceOf(user), 1000 * UNIT - 24 * UNIT);
        assertEq(token.allowance(user, address(hunt)), 0);
        assertEq(token.balanceOf(address(hunt)), 0);
    }

    function test_TakesTheWholeRange() public {
        PlainToken token = new PlainToken();
        HuntSponsor hunt = _funded(token, type(uint256).max);
        vm.startPrank(user);
        hunt.sponsor(4, 3, 2, 1);
        hunt.sponsor(255, 254, 3, MAX_HOURS);
        vm.stopPrank();

        assertEq(token.balanceOf(DEAD), (1 + MAX_HOURS) * UNIT);
        assertEq(token.balanceOf(address(hunt)), 0);
    }

    function test_AcceptsTokenThatReturnsNothing() public {
        SilentToken token = new SilentToken();
        HuntSponsor hunt = _funded(token, type(uint256).max);
        vm.expectEmit(address(hunt));
        emit HuntSponsor.Sponsored(user, 41, 9, 3, 5 * UNIT, 5);
        vm.prank(user);
        hunt.sponsor(41, 9, 3, 5);

        assertEq(token.balanceOf(DEAD), 5 * UNIT);
        assertEq(token.balanceOf(user), 1000 * UNIT - 5 * UNIT);
        assertEq(token.balanceOf(address(hunt)), 0);
    }

    function test_RejectsTokenThatReturnsFalse() public {
        FalseToken token = new FalseToken();
        HuntSponsor hunt = _funded(token, type(uint256).max);
        vm.expectRevert(HuntSponsor.TransferFailed.selector);
        vm.prank(user);
        hunt.sponsor(83, 17, 2, 1);
        assertEq(token.balanceOf(address(hunt)), 0);
    }

    function test_RejectsMissingApproval() public {
        PlainToken plain = new PlainToken();
        SilentToken silent = new SilentToken();
        HuntSponsor plainHunt = _funded(plain, 0);
        HuntSponsor silentHunt = _funded(silent, UNIT - 1);

        vm.startPrank(user);
        vm.expectRevert(HuntSponsor.TransferFailed.selector);
        plainHunt.sponsor(83, 17, 2, 1);
        vm.expectRevert(HuntSponsor.TransferFailed.selector);
        silentHunt.sponsor(83, 17, 2, 1);
        vm.stopPrank();

        assertEq(plain.balanceOf(DEAD) + silent.balanceOf(DEAD), 0);
        assertEq(plain.balanceOf(address(plainHunt)) + silent.balanceOf(address(silentHunt)), 0);
    }

    function test_RejectsTokenWithoutCode() public {
        HuntSponsor hunt = new HuntSponsor(makeAddr("no code"), UNIT, MAX_HOURS);
        vm.expectRevert(HuntSponsor.TransferFailed.selector);
        vm.prank(user);
        hunt.sponsor(83, 17, 2, 1);
    }

    function test_RejectsBadEntries() public {
        PlainToken token = new PlainToken();
        HuntSponsor hunt = _funded(token, type(uint256).max);
        _expectBadEntry(hunt, 2, 3, 2);
        _expectBadEntry(hunt, 256, 17, 2);
        _expectBadEntry(hunt, 83, 17, 0);
        _expectBadEntry(hunt, 83, 17, 1);
        _expectBadEntry(hunt, 83, 17, 4);
        _expectBadEntry(hunt, 83, 2, 2);
        _expectBadEntry(hunt, 83, 3, 3);
        _expectBadEntry(hunt, 83, 2, 3);
        _expectBadEntry(hunt, 17, 17, 2);
        _expectBadEntry(hunt, 17, 18, 2);
        assertEq(token.balanceOf(DEAD), 0);
        assertEq(token.balanceOf(address(hunt)), 0);
    }

    function test_RejectsBadHours() public {
        PlainToken token = new PlainToken();
        HuntSponsor hunt = _funded(token, type(uint256).max);
        vm.startPrank(user);
        vm.expectRevert(abi.encodeWithSelector(HuntSponsor.InvalidHours.selector, 0));
        hunt.sponsor(83, 17, 2, 0);
        vm.expectRevert(abi.encodeWithSelector(HuntSponsor.InvalidHours.selector, MAX_HOURS + 1));
        hunt.sponsor(83, 17, 2, MAX_HOURS + 1);
        vm.stopPrank();
        assertEq(token.balanceOf(DEAD), 0);
        assertEq(token.balanceOf(address(hunt)), 0);
    }

    function test_ConstructorRejectsZero() public {
        vm.expectRevert(HuntSponsor.ZeroArgument.selector);
        new HuntSponsor(address(0), UNIT, MAX_HOURS);
        vm.expectRevert(HuntSponsor.ZeroArgument.selector);
        new HuntSponsor(address(1), 0, MAX_HOURS);
        vm.expectRevert(HuntSponsor.ZeroArgument.selector);
        new HuntSponsor(address(1), UNIT, 0);
    }

    /// A sponsor contract for `token`, with 1000 hours' worth minted to the user and `allowance` approved.
    function _funded(Ledger token, uint256 allowance) internal returns (HuntSponsor hunt) {
        hunt = new HuntSponsor(address(token), UNIT, MAX_HOURS);
        token.mint(user, 1000 * UNIT);
        vm.prank(user);
        token.approve(address(hunt), allowance);
    }

    function _expectBadEntry(HuntSponsor hunt, uint16 v, uint8 k, uint8 t) internal {
        vm.expectRevert(abi.encodeWithSelector(HuntSponsor.InvalidEntry.selector, v, k, t));
        vm.prank(user);
        hunt.sponsor(v, k, t, 1);
    }
}
