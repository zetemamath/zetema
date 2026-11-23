// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity 0.8.28;

/// @dev The ERC-20 functions the zetema contracts call.
interface IERC20 {
    /// @notice The tokens `account` holds.
    function balanceOf(address account) external view returns (uint256);

    /// @notice Moves `amount` of the caller's tokens to `to`.
    function transfer(address to, uint256 amount) external returns (bool);

    /// @notice Moves `amount` of `from`'s tokens to `to`, out of what `from` has approved the caller for.
    function transferFrom(address from, address to, uint256 amount) external returns (bool);
}
