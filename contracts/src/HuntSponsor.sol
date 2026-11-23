// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity 0.8.28;

/// @dev The one ERC-20 function this contract calls.
interface IERC20 {
    function transferFrom(address from, address to, uint256 amount) external returns (bool);
}

/// @title HuntSponsor
/// @notice Buys hours of search on a covering entry C(v, k, t) by burning a token: each hour costs `unit` tokens,
/// moved from the sponsor straight to the burn address. The contract keeps no state and never holds tokens; the
/// `Sponsored` events are the record.
contract HuntSponsor {
    /// @notice Where the tokens go.
    address public constant BURN = 0x000000000000000000000000000000000000dEaD;

    /// @notice The token that is burned.
    address public immutable token;
    /// @notice The token amount, in the token's smallest units, that buys one hour.
    uint256 public immutable unit;
    /// @notice The most hours one call can buy.
    uint256 public immutable maxHours;

    /// @notice `sponsor` burned `amount` tokens for `hours_` hours of search on C(v, k, t).
    event Sponsored(address indexed sponsor, uint16 v, uint8 k, uint8 t, uint256 amount, uint256 hours_);

    /// @notice A constructor argument is zero.
    error ZeroArgument();
    /// @notice Not an entry the search takes: it needs 3 <= v <= 255, t of 2 or 3, and t < k < v.
    error InvalidEntry(uint16 v, uint8 k, uint8 t);
    /// @notice hours_ is zero or above maxHours.
    error InvalidHours(uint256 hours_);
    /// @notice The transfer reverted, returned false, or the token address has no code.
    error TransferFailed();

    /// @param token_ The token to burn.
    /// @param unit_ The token amount, in the token's smallest units, that buys one hour.
    /// @param maxHours_ The most hours one call can buy.
    constructor(address token_, uint256 unit_, uint256 maxHours_) {
        if (token_ == address(0) || unit_ == 0 || maxHours_ == 0) revert ZeroArgument();
        token = token_;
        unit = unit_;
        maxHours = maxHours_;
    }

    /// @notice Burns `hours_ * unit` of the caller's tokens for `hours_` hours of search on C(v, k, t). The caller
    /// must have approved this contract for at least that amount.
    /// @param v Number of points, 3 to 255.
    /// @param k Points per block, above t and below v.
    /// @param t Size of the sets to cover, 2 or 3.
    /// @param hours_ Hours to buy, 1 to maxHours.
    function sponsor(uint16 v, uint8 k, uint8 t, uint256 hours_) external {
        if (v < 3 || v > 255 || (t != 2 && t != 3) || k <= t || k >= v) revert InvalidEntry(v, k, t);
        if (hours_ == 0 || hours_ > maxHours) revert InvalidHours(hours_);
        uint256 amount = hours_ * unit;
        (bool ok, bytes memory data) = token.call(abi.encodeCall(IERC20.transferFrom, (msg.sender, BURN, amount)));
        // Some tokens return nothing from transferFrom. So does an address without code, which must not pass.
        if (!ok || (data.length == 0 ? token.code.length == 0 : !abi.decode(data, (bool)))) revert TransferFailed();
        emit Sponsored(msg.sender, v, k, t, amount, hours_);
    }
}
