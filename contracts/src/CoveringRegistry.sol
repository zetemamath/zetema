// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity 0.8.28;

/// @title CoveringRegistry
/// @notice Records, for each (v, k), the (v, k, 2) covering design with the fewest blocks submitted so far. Every
/// submission is checked in full on chain: its blocks must hold every pair of the v points.
/// @dev Only the size, finder, sponsor, time and id are stored. The blocks are not: they are in the submitting call's
/// data and in its `Recorded` event, and the id commits to them. Points count from 0 and take one byte each; the
/// certificate text format counts them from 1.
contract CoveringRegistry {
    struct Record {
        uint32 blocks; // 0 when there is no record
        uint64 timestamp;
        address finder;
        address sponsor; // as named by the finder, not checked
        bytes32 id;
    }

    mapping(uint16 v => mapping(uint8 k => Record)) private _records;

    /// @notice A covering of `count` blocks became the record for (v, k). `blocks` holds the design exactly as
    /// submitted, so the logs alone are enough to rebuild every record.
    event Recorded(
        uint16 indexed v,
        uint8 indexed k,
        uint32 count,
        address indexed finder,
        address sponsor,
        bytes32 id,
        bytes blocks
    );

    /// @notice v is not in [3, 255] or k is not in [2, v - 1].
    error InvalidParameters(uint16 v, uint8 k);
    /// @notice The design is empty or its length is not a multiple of k.
    error InvalidLength(uint256 length);
    /// @notice The byte at `offset` names a point that is not below v.
    error PointOutOfRange(uint256 offset);
    /// @notice The byte at `offset` is not above the byte before it in its block.
    error PointsNotIncreasing(uint256 offset);
    /// @notice Block `index` (from 0) does not come strictly after the block before it in lexicographic order.
    error BlocksNotIncreasing(uint256 index);
    /// @notice No block holds both x and y (x < y, from 0); of all such pairs this one has the lowest index.
    error PairNotCovered(uint256 x, uint256 y);
    /// @notice The design has `blocks` blocks and the record for (v, k) has `record`, which is not more.
    error NotSmaller(uint32 blocks, uint32 record);

    /// @notice Checks a (v, k, 2) covering design and, if it has fewer blocks than the record for (v, k) or there is
    /// no record yet, stores it as the new record with the caller as finder.
    /// @param v Number of points, 3 to 255.
    /// @param k Points per block, 2 to v - 1.
    /// @param blocks The blocks one after another, k bytes each, one byte per point (from 0). Points must ascend
    /// within a block and blocks must ascend in lexicographic order, so a design has exactly one encoding.
    /// @param sponsor Whoever the finder credits for the search. Not checked; may be zero.
    /// @return id keccak256(abi.encodePacked(v, k, blocks)).
    function submit(uint16 v, uint8 k, bytes calldata blocks, address sponsor) external returns (bytes32 id) {
        if (v < 3 || v > 255 || k < 2 || k >= v) revert InvalidParameters(v, k);
        uint256 length = blocks.length;
        if (length == 0 || length % k != 0 || length / k > type(uint32).max) revert InvalidLength(length);
        // Safe: the line above keeps length / k within uint32.
        // forge-lint: disable-next-line(unsafe-typecast)
        uint32 count = uint32(length / k);
        uint32 best = _records[v][k].blocks;
        if (best != 0 && count >= best) revert NotSmaller(count, best);

        _check(v, k, blocks);

        id = keccak256(abi.encodePacked(v, k, blocks));
        // Safe: block times stay within uint64 for the next 500 billion years.
        // forge-lint: disable-next-line(unsafe-typecast)
        uint64 time = uint64(block.timestamp);
        _records[v][k] = Record({blocks: count, timestamp: time, finder: msg.sender, sponsor: sponsor, id: id});
        emit Recorded(v, k, count, msg.sender, sponsor, id, blocks);
    }

    /// @notice The record for (v, k). Every field is zero when there is none.
    /// @param v Number of points.
    /// @param k Points per block.
    /// @return The number of blocks, the block time of the submission, the finder, the sponsor and the id.
    function recordOf(uint16 v, uint8 k) external view returns (Record memory) {
        return _records[v][k];
    }

    /// @dev Reverts unless `blocks` is canonical and covers every pair. The pair {x, y} with x < y has index
    /// y * (y - 1) / 2 + x, its place in colexicographic order, and one bit in `covered`.
    function _check(uint256 v, uint256 k, bytes calldata blocks) private pure {
        uint256 pairs = v * (v - 1) / 2;
        uint256[] memory covered = new uint256[]((pairs + 255) / 256);
        // Offsets stay below blocks.length and pair indexes below C(255, 2), and each subtraction is guarded by the
        // branch it sits in, so nothing here can wrap.
        unchecked {
            for (uint256 start = 0; start < blocks.length; start += k) {
                // Whether this block already differs from the one before it; the first block has none before it.
                bool ordered = start == 0;
                for (uint256 at = start; at < start + k; ++at) {
                    uint256 y = uint8(blocks[at]);
                    if (y >= v) revert PointOutOfRange(at);
                    if (at != start) {
                        if (y <= uint8(blocks[at - 1])) revert PointsNotIncreasing(at);
                        uint256 row = y * (y - 1) / 2;
                        for (uint256 j = start; j < at; ++j) {
                            uint256 pair = row + uint8(blocks[j]);
                            covered[pair / 256] |= 1 << (pair % 256);
                        }
                    }
                    if (!ordered) {
                        uint256 prior = uint8(blocks[at - k]);
                        if (y < prior) revert BlocksNotIncreasing(start / k);
                        ordered = y > prior;
                    }
                }
                if (!ordered) revert BlocksNotIncreasing(start / k);
            }
        }

        uint256 last = covered.length - 1;
        for (uint256 word; word <= last; ++word) {
            uint256 full = word == last && pairs % 256 != 0 ? (1 << (pairs % 256)) - 1 : type(uint256).max;
            uint256 missing = full & ~covered[word];
            if (missing != 0) {
                uint256 pair = word * 256;
                while (missing & 1 == 0) {
                    missing >>= 1;
                    ++pair;
                }
                uint256 y = 1;
                while (y * (y + 1) / 2 <= pair) {
                    ++y;
                }
                revert PairNotCovered(pair - y * (y - 1) / 2, y);
            }
        }
    }
}
