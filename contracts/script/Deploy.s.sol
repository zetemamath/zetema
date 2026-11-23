// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity 0.8.28;

import {Script} from "forge-std/Script.sol";
import {BountyBoard} from "../src/BountyBoard.sol";
import {CoveringRegistry} from "../src/CoveringRegistry.sol";
import {HuntSponsor} from "../src/HuntSponsor.sol";
import {RECORD_001} from "./Record001.sol";

/// @title Deploy
/// @notice Deploys the zetema contracts in two steps: first the registry, with record 001 in it; then, once the token
/// exists, the sponsor and the bounty board for that token.
contract Deploy is Script {
    /// @notice Deploys the registry and submits record 001, C(83,17,2) = 30, right after it in the same broadcast, with
    /// the broadcasting account as finder and no sponsor. The certificate is already public, so the lab's submission
    /// has to be the first one.
    /// @return deployed The registry.
    function registry() external returns (CoveringRegistry deployed) {
        vm.startBroadcast();
        deployed = new CoveringRegistry();
        deployed.submit(83, 17, RECORD_001, address(0));
        vm.stopBroadcast();
    }

    /// @notice Deploys HuntSponsor and BountyBoard for `token`.
    /// @param registry_ The registry whose records earn the bounty board's pots.
    /// @param token The token that sponsors burn and the pots hold.
    /// @param unit The token amount, in the token's smallest units, that buys one hour of search.
    /// @param maxHours The most hours one sponsor call can buy.
    /// @return sponsor The sponsor contract.
    /// @return board The bounty board.
    function tokenContracts(CoveringRegistry registry_, address token, uint256 unit, uint256 maxHours)
        external
        returns (HuntSponsor sponsor, BountyBoard board)
    {
        vm.startBroadcast();
        sponsor = new HuntSponsor(token, unit, maxHours);
        board = new BountyBoard(registry_, token);
        vm.stopBroadcast();
    }
}
