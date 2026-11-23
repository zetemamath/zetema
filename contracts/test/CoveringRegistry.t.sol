// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity 0.8.28;

import {Test, console} from "forge-std/Test.sol";
import {RECORD_001 as RECORD, RECORD_001_SHA256 as RECORD_SHA256} from "../script/Record001.sol";
import {CoveringRegistry} from "../src/CoveringRegistry.sol";

contract CoveringRegistryTest is Test {
    // The Fano plane, C(7, 3, 2) = 7, one block per hex string.
    bytes constant FANO = hex"000102" hex"000304" hex"000506" hex"010305" hex"010406" hex"020306" hex"020405";

    CoveringRegistry registry;
    address finder = makeAddr("finder");
    address sponsor = makeAddr("sponsor");

    function setUp() public {
        registry = new CoveringRegistry();
    }

    function test_AcceptsTheFanoPlane() public {
        bytes32 id = keccak256(abi.encodePacked(uint16(7), uint8(3), FANO));
        vm.expectEmit(address(registry));
        emit CoveringRegistry.Recorded(7, 3, 7, finder, sponsor, id, FANO);
        vm.prank(finder);
        uint256 left = gasleft();
        bytes32 returned = registry.submit(7, 3, FANO, sponsor);
        console.log("gas, C(7,3,2) = 7, the Fano plane:", left - gasleft());

        assertEq(returned, id);
        CoveringRegistry.Record memory record = registry.recordOf(7, 3);
        assertEq(record.blocks, 7);
        assertEq(record.timestamp, block.timestamp);
        assertEq(record.finder, finder);
        assertEq(record.sponsor, sponsor);
        assertEq(record.id, id);
    }

    function test_AcceptsTheRecord() public {
        // The log carries the blocks exactly as submitted, so the record can be rebuilt from logs alone.
        vm.expectEmit(address(registry));
        emit CoveringRegistry.Recorded(
            83, 17, 30, finder, address(0), keccak256(abi.encodePacked(uint16(83), uint8(17), RECORD)), RECORD
        );
        vm.prank(finder);
        uint256 left = gasleft();
        bytes32 id = registry.submit(83, 17, RECORD, address(0));
        uint256 used = left - gasleft();
        console.log("gas, C(83,17,2) = 30:", used);

        assertLt(used, 3_000_000);
        CoveringRegistry.Record memory record = registry.recordOf(83, 17);
        assertEq(record.blocks, 30);
        assertEq(record.finder, finder);
        assertEq(record.id, keccak256(abi.encodePacked(uint16(83), uint8(17), RECORD)));
        assertEq(record.id, id);
    }

    function test_RecordIsThePublishedCertificate() public pure {
        assertEq(sha256(_certificate(83, 17, RECORD)), RECORD_SHA256);
    }

    function test_AcceptsTheLargestPointCount() public {
        // Three blocks of 254 points, leaving out 2, 1 and 0 in turn, hold every pair of 255 points.
        registry.submit(255, 254, bytes.concat(_allBut(255, 2), _allBut(255, 1), _allBut(255, 0)), address(0));
        assertEq(registry.recordOf(255, 254).blocks, 3);
    }

    function test_NamesTheFirstUncoveredPair() public {
        // The Fano plane without {0, 3, 4} misses {0, 3}, {0, 4} and {3, 4}; {0, 3} has the lowest index.
        _expectUncovered(7, 3, _drop(FANO, 3, 1), 0, 3);
        // The record without block 14 first misses {2, 27}, whose bit lies past the first 256-bit word.
        _expectUncovered(83, 17, _drop(RECORD, 17, 14), 2, 27);
        // Only the first pair is missing; then only the last, in a bitmap of one short word and in the largest bitmap.
        _expectUncovered(4, 3, hex"000203" hex"010203", 0, 1);
        _expectUncovered(4, 3, hex"000102" hex"000103", 2, 3);
        _expectUncovered(255, 254, bytes.concat(_allBut(255, 254), _allBut(255, 253)), 253, 254);
    }

    function test_RejectsParametersOutOfRange() public {
        _expectError(2, 2, FANO, abi.encodeWithSelector(CoveringRegistry.InvalidParameters.selector, 2, 2));
        _expectError(256, 3, FANO, abi.encodeWithSelector(CoveringRegistry.InvalidParameters.selector, 256, 3));
        _expectError(7, 0, FANO, abi.encodeWithSelector(CoveringRegistry.InvalidParameters.selector, 7, 0));
        _expectError(7, 1, FANO, abi.encodeWithSelector(CoveringRegistry.InvalidParameters.selector, 7, 1));
        _expectError(7, 7, FANO, abi.encodeWithSelector(CoveringRegistry.InvalidParameters.selector, 7, 7));
        _expectError(7, 8, FANO, abi.encodeWithSelector(CoveringRegistry.InvalidParameters.selector, 7, 8));
    }

    function test_RejectsWrongLength() public {
        _expectError(7, 3, "", abi.encodeWithSelector(CoveringRegistry.InvalidLength.selector, 0));
        _expectError(7, 3, _slice(FANO, 0, 20), abi.encodeWithSelector(CoveringRegistry.InvalidLength.selector, 20));
    }

    function test_RejectsPointOutOfRange() public {
        // The last block {2, 4, 5} becomes {2, 4, 7}, and 7 is not one of the points 0 to 6.
        bytes memory blocks = bytes.concat(_slice(FANO, 0, 18), hex"020407");
        _expectError(7, 3, blocks, abi.encodeWithSelector(CoveringRegistry.PointOutOfRange.selector, 20));
    }

    function test_RejectsPointsNotIncreasing() public {
        // The first block {0, 1, 2} becomes {0, 2, 1}, then {0, 1, 1}.
        bytes memory swapped = bytes.concat(hex"000201", _slice(FANO, 3, 18));
        bytes memory repeated = bytes.concat(hex"000101", _slice(FANO, 3, 18));
        _expectError(7, 3, swapped, abi.encodeWithSelector(CoveringRegistry.PointsNotIncreasing.selector, 2));
        _expectError(7, 3, repeated, abi.encodeWithSelector(CoveringRegistry.PointsNotIncreasing.selector, 2));
    }

    function test_RejectsBlocksNotIncreasing() public {
        // The first two blocks swapped; the last two swapped, so {2, 3, 6} follows {2, 4, 5}: smaller at the first
        // point that differs, larger at the last; then the last block given twice.
        bytes memory swapped = bytes.concat(_slice(FANO, 3, 3), _slice(FANO, 0, 3), _slice(FANO, 6, 15));
        bytes memory swappedLast = bytes.concat(_slice(FANO, 0, 15), hex"020405", hex"020306");
        bytes memory repeated = bytes.concat(FANO, hex"020405");
        _expectError(7, 3, swapped, abi.encodeWithSelector(CoveringRegistry.BlocksNotIncreasing.selector, 1));
        _expectError(7, 3, swappedLast, abi.encodeWithSelector(CoveringRegistry.BlocksNotIncreasing.selector, 6));
        _expectError(7, 3, repeated, abi.encodeWithSelector(CoveringRegistry.BlocksNotIncreasing.selector, 7));
    }

    function test_KeepsOnlyFewerBlocks() public {
        // The Fano plane with {0, 1, 3} added in its place: a covering with 8 blocks.
        bytes memory larger = bytes.concat(_slice(FANO, 0, 3), hex"000103", _slice(FANO, 3, 18));
        registry.submit(7, 3, larger, sponsor);
        assertEq(registry.recordOf(7, 3).blocks, 8);

        vm.prank(finder);
        bytes32 id = registry.submit(7, 3, FANO, address(0));
        CoveringRegistry.Record memory record = registry.recordOf(7, 3);
        assertEq(record.blocks, 7);
        assertEq(record.finder, finder);
        assertEq(record.sponsor, address(0));
        assertEq(record.id, id);

        // The same design again, another Fano plane (the cyclic one from {0, 1, 3}) and the larger one all fail.
        bytes memory cyclic = hex"000103" hex"000206" hex"000405" hex"010204" hex"010506" hex"020305" hex"030406";
        _expectError(7, 3, FANO, abi.encodeWithSelector(CoveringRegistry.NotSmaller.selector, 7, 7));
        _expectError(7, 3, cyclic, abi.encodeWithSelector(CoveringRegistry.NotSmaller.selector, 7, 7));
        _expectError(7, 3, larger, abi.encodeWithSelector(CoveringRegistry.NotSmaller.selector, 8, 7));
        assertEq(registry.recordOf(7, 3).id, id);
    }

    /// Random canonical designs: the registry must agree with the plain check below, down to the pair it names.
    function testFuzz_MatchesReference(uint256 seed) public {
        (uint16 v, uint8 k, bytes memory blocks) = _randomDesign(seed);
        assertTrue(_canonical(v, k, blocks));
        (bool covers, uint256 x, uint256 y) = _firstUncovered(v, k, blocks);
        if (!covers) vm.expectRevert(abi.encodeWithSelector(CoveringRegistry.PairNotCovered.selector, x, y));
        registry.submit(v, k, blocks, address(0));
        if (covers) assertEq(registry.recordOf(v, k).blocks, blocks.length / k);
    }

    /// The same designs with one byte replaced, which can break a range, an order or the covering, or change nothing.
    function testFuzz_MutationsMatchReference(uint256 seed, uint256 at, uint8 value) public {
        (uint16 v, uint8 k, bytes memory blocks) = _randomDesign(seed);
        blocks[at % blocks.length] = bytes1(value % 16);
        (bool covers,,) = _firstUncovered(v, k, blocks);
        (bool accepted,) = address(registry).call(abi.encodeCall(CoveringRegistry.submit, (v, k, blocks, address(0))));
        assertEq(accepted, covers && _canonical(v, k, blocks));
    }

    function _expectError(uint16 v, uint8 k, bytes memory blocks, bytes memory error_) internal {
        vm.expectRevert(error_);
        registry.submit(v, k, blocks, address(0));
    }

    function _expectUncovered(uint16 v, uint8 k, bytes memory blocks, uint256 x, uint256 y) internal {
        _expectError(v, k, blocks, abi.encodeWithSelector(CoveringRegistry.PairNotCovered.selector, x, y));
    }

    function _slice(bytes memory data, uint256 start, uint256 length) internal pure returns (bytes memory part) {
        part = new bytes(length);
        for (uint256 i; i < length; ++i) {
            part[i] = data[start + i];
        }
    }

    function _drop(bytes memory blocks, uint256 k, uint256 index) internal pure returns (bytes memory) {
        uint256 end = (index + 1) * k;
        return bytes.concat(_slice(blocks, 0, index * k), _slice(blocks, end, blocks.length - end));
    }

    function _allBut(uint256 v, uint256 skip) internal pure returns (bytes memory points) {
        points = new bytes(v - 1);
        uint256 n;
        for (uint256 p; p < v; ++p) {
            if (p != skip) points[n++] = bytes1(uint8(p));
        }
    }

    /// The certificate text of a (v, k, 2) design, in the canonical form whose SHA-256 the lab publishes.
    function _certificate(uint256 v, uint256 k, bytes memory blocks) internal pure returns (bytes memory text) {
        text = abi.encodePacked("zetema/1 covering ", vm.toString(v), " ", vm.toString(k), " 2\n");
        for (uint256 i; i < blocks.length; ++i) {
            bytes1 end = (i + 1) % k == 0 ? bytes1("\n") : bytes1(" ");
            text = abi.encodePacked(text, vm.toString(uint256(uint8(blocks[i])) + 1), end);
        }
    }

    /// A random canonical design on 3 to 12 points, with blocks of 2 to 5 points and 1 to 16 blocks.
    function _randomDesign(uint256 seed) internal pure returns (uint16 v, uint8 k, bytes memory blocks) {
        v = uint16(3 + seed % 10);
        k = uint8(2 + (seed >> 8) % (v < 6 ? v - 2 : 4));
        uint256 count = 1 + (seed >> 16) % 16;
        bytes[] memory list = new bytes[](count);
        for (uint256 i; i < count; ++i) {
            list[i] = _randomBlock(uint256(keccak256(abi.encode(seed, i))), v, k);
        }
        for (uint256 i = 1; i < count; ++i) {
            for (uint256 j = i; j > 0 && _less(list[j], list[j - 1]); --j) {
                (list[j], list[j - 1]) = (list[j - 1], list[j]);
            }
        }
        for (uint256 i; i < count; ++i) {
            if (i == 0 || keccak256(list[i]) != keccak256(list[i - 1])) blocks = bytes.concat(blocks, list[i]);
        }
    }

    /// k of the v points, uniformly at random and in ascending order: keep each point with chance needed / left.
    function _randomBlock(uint256 r, uint256 v, uint256 k) internal pure returns (bytes memory points) {
        points = new bytes(k);
        uint256 need = k;
        for (uint256 p; need > 0; ++p) {
            if (r % (v - p) < need) {
                points[k - need] = bytes1(uint8(p));
                --need;
            }
            r /= v - p;
        }
    }

    function _less(bytes memory a, bytes memory b) internal pure returns (bool) {
        for (uint256 i; i < a.length; ++i) {
            if (a[i] != b[i]) return a[i] < b[i];
        }
        return false;
    }

    /// The format rules, checked one at a time.
    function _canonical(uint256 v, uint256 k, bytes memory blocks) internal pure returns (bool) {
        if (blocks.length == 0 || blocks.length % k != 0) return false;
        for (uint256 i; i < blocks.length; ++i) {
            if (uint8(blocks[i]) >= v) return false;
            if (i % k != 0 && blocks[i] <= blocks[i - 1]) return false;
        }
        for (uint256 start = k; start < blocks.length; start += k) {
            if (!_less(_slice(blocks, start - k, k), _slice(blocks, start, k))) return false;
        }
        return true;
    }

    /// The pair with the lowest index that no block holds, found by looking for both points in every block.
    function _firstUncovered(uint256 v, uint256 k, bytes memory blocks)
        internal
        pure
        returns (bool covers, uint256 x, uint256 y)
    {
        for (y = 1; y < v; ++y) {
            for (x = 0; x < y; ++x) {
                if (!_held(blocks, k, x, y)) return (false, x, y);
            }
        }
        return (true, 0, 0);
    }

    function _held(bytes memory blocks, uint256 k, uint256 x, uint256 y) internal pure returns (bool) {
        for (uint256 start; start < blocks.length; start += k) {
            bool hasX;
            bool hasY;
            for (uint256 i = start; i < start + k; ++i) {
                hasX = hasX || uint8(blocks[i]) == x;
                hasY = hasY || uint8(blocks[i]) == y;
            }
            if (hasX && hasY) return true;
        }
        return false;
    }
}
