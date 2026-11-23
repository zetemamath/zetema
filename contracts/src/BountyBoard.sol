// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity 0.8.28;

import {CoveringRegistry} from "./CoveringRegistry.sol";
import {IERC20} from "./IERC20.sol";

/// @title BountyBoard
/// @notice Token bounties on covering records. A pot sits on a target, a (v, k, 2) covering of at most maxBlocks
/// blocks. Anyone can add to it, and once the registry's record for (v, k) meets the target, anyone can call `claim`,
/// which sends the whole pot to that record's finder. The registry checks every record on chain, so no one judges a
/// claim. There is no owner, no admin and no upgrade.
/// @dev The pot goes to whoever holds the record when `claim` is called. If two records that meet the target land
/// before a claim, the finder of the later one is paid and the earlier one gets nothing. The lab calls `claim` right
/// after every new record. Funders can never take tokens back, and a pot that no one earns stays locked here: funding
/// takes tokens out of circulation until someone earns them. Tokens sent here other than through `fund` belong to no
/// pot and stay here for good.
contract BountyBoard {
    /// @notice The registry whose records earn the pots.
    CoveringRegistry public immutable registry;
    /// @notice The token the pots hold.
    address public immutable token;

    mapping(uint16 v => mapping(uint8 k => mapping(uint32 maxBlocks => uint256))) private _pots;
    bool private transient _entered;

    /// @notice `funder` added `amount` tokens to the pot for a (v, k, 2) covering of at most maxBlocks blocks, which
    /// now holds `pot`.
    event Funded(
        address indexed funder, uint16 indexed v, uint8 indexed k, uint32 maxBlocks, uint256 amount, uint256 pot
    );
    /// @notice The pot for a (v, k, 2) covering of at most maxBlocks blocks, `amount` tokens, was sent to `finder`, the
    /// finder of the record `recordId`.
    event Paid(
        address indexed finder, uint16 indexed v, uint8 indexed k, uint32 maxBlocks, uint256 amount, bytes32 recordId
    );

    /// @notice A constructor argument has no code.
    error NotAContract(address account);
    /// @notice v is not in [3, 255] or k is not in [2, v - 1], as in the registry.
    error InvalidParameters(uint16 v, uint8 k);
    /// @notice The amount is zero.
    error ZeroAmount();
    /// @notice No (v, k, 2) covering has as few as `maxBlocks` blocks: the Schoenheim bound is `lowerBound`.
    error ImpossibleTarget(uint32 maxBlocks, uint32 lowerBound);
    /// @notice The registry's record for (v, k), of `record` blocks, already meets the target.
    error TargetAlreadyMet(uint32 maxBlocks, uint32 record);
    /// @notice The pot is empty.
    error NoPot();
    /// @notice The registry's record for (v, k), of `record` blocks (0 when there is none), does not meet the target.
    error TargetNotMet(uint32 maxBlocks, uint32 record);
    /// @notice The token call reverted or returned false.
    error TransferFailed();
    /// @notice `fund` or `claim` was called from inside a token call made by one of them.
    error Reentered();

    /// @dev `fund` credits the change in this contract's balance across its transfer, which a `fund` or `claim` made
    /// from inside that transfer would change as well.
    modifier nonReentrant() {
        if (_entered) revert Reentered();
        _entered = true;
        _;
        _entered = false;
    }

    /// @param registry_ The registry whose records earn the pots.
    /// @param token_ The token the pots hold.
    // The zero address has no code, so the checks below reject it too.
    // forge-lint: disable-next-line(missing-zero-check)
    constructor(CoveringRegistry registry_, address token_) {
        // A call to an account without code succeeds and returns nothing, as a transfer by a token that returns
        // nothing does, so the token calls below rely on this check.
        if (address(registry_).code.length == 0) revert NotAContract(address(registry_));
        if (token_.code.length == 0) revert NotAContract(token_);
        registry = registry_;
        token = token_;
    }

    /// @notice Adds `amount` of the caller's tokens to the pot for a (v, k, 2) covering of at most maxBlocks blocks.
    /// The caller must have approved this contract for `amount` and can never take the tokens back. The pot grows by
    /// what arrives, which is less than `amount` if the token charges a fee on transfers.
    /// @param v Number of points, 3 to 255.
    /// @param k Points per block, 2 to v - 1.
    /// @param maxBlocks The target. It must be at least the Schoenheim bound for (v, k), and below the registry's
    /// record for (v, k) if there is one.
    /// @param amount Tokens to add, in the token's smallest units.
    function fund(uint16 v, uint8 k, uint32 maxBlocks, uint256 amount) external nonReentrant {
        if (v < 3 || v > 255 || k < 2 || k >= v) revert InvalidParameters(v, k);
        if (amount == 0) revert ZeroAmount();
        uint32 bound = _lowerBound(v, k);
        if (maxBlocks < bound) revert ImpossibleTarget(maxBlocks, bound);
        uint32 record = registry.recordOf(v, k).blocks;
        if (record != 0 && record <= maxBlocks) revert TargetAlreadyMet(maxBlocks, record);

        uint256 before = IERC20(token).balanceOf(address(this));
        _call(abi.encodeCall(IERC20.transferFrom, (msg.sender, address(this), amount)));
        uint256 received = IERC20(token).balanceOf(address(this)) - before;
        uint256 pot = _pots[v][k][maxBlocks] + received;
        _pots[v][k][maxBlocks] = pot;
        // Only known after the transfer. The lock keeps the token from calling back in, so no log can come between.
        // forge-lint: disable-next-line(reentrancy-events)
        emit Funded(msg.sender, v, k, maxBlocks, received, pot);
    }

    /// @notice Sends the whole pot for a (v, k, 2) covering of at most maxBlocks blocks to the finder of the
    /// registry's record for (v, k), if that record has at most maxBlocks blocks. Anyone may call it; the tokens go to
    /// the finder, not to the caller.
    /// @param v Number of points.
    /// @param k Points per block.
    /// @param maxBlocks The target.
    function claim(uint16 v, uint8 k, uint32 maxBlocks) external nonReentrant {
        uint256 pot = _pots[v][k][maxBlocks];
        if (pot == 0) revert NoPot();
        CoveringRegistry.Record memory record = registry.recordOf(v, k);
        if (record.blocks == 0 || record.blocks > maxBlocks) revert TargetNotMet(maxBlocks, record.blocks);
        delete _pots[v][k][maxBlocks];
        emit Paid(record.finder, v, k, maxBlocks, pot, record.id);
        _call(abi.encodeCall(IERC20.transfer, (record.finder, pot)));
    }

    /// @notice The tokens in the pot for a (v, k, 2) covering of at most maxBlocks blocks.
    /// @param v Number of points.
    /// @param k Points per block.
    /// @param maxBlocks The target.
    /// @return The pot, in the token's smallest units.
    function potOf(uint16 v, uint8 k, uint32 maxBlocks) external view returns (uint256) {
        return _pots[v][k][maxBlocks];
    }

    /// @dev Reverts unless the token call succeeded and returned true or nothing, as some tokens do.
    function _call(bytes memory data) private {
        // The one state written after this call is the lock's own release.
        // forge-lint: disable-next-line(reentrancy-no-eth)
        (bool ok, bytes memory returned) = token.call(data);
        if (!ok || (returned.length != 0 && !abi.decode(returned, (bool)))) revert TransferFailed();
    }

    /// @dev The Schoenheim bound L(v, k, 2) = ceil(v / k * ceil((v - 1) / (k - 1))): no (v, k, 2) covering has fewer
    /// blocks. With 2 <= k < v <= 255 nothing here can wrap or divide by zero.
    function _lowerBound(uint32 v, uint32 k) private pure returns (uint32) {
        // Each point has v - 1 others to meet, at most k - 1 per block, so it lies in at least this many blocks; the
        // blocks hold k points each.
        uint32 perPoint = (v + k - 3) / (k - 1);
        // Both divisions round up, as the bound does.
        // forge-lint: disable-next-line(divide-before-multiply)
        return (v * perPoint + k - 1) / k;
    }
}
