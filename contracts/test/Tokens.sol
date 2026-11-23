// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity 0.8.28;

/// Balances and allowances for the test tokens below, which differ only in what transfer and transferFrom return.
abstract contract Ledger {
    mapping(address => uint256) public balanceOf;
    mapping(address => mapping(address => uint256)) public allowance;

    function mint(address to, uint256 amount) external {
        balanceOf[to] += amount;
    }

    function approve(address spender, uint256 amount) external returns (bool) {
        allowance[msg.sender][spender] = amount;
        return true;
    }

    /// Reverts on a missing allowance or balance, by underflow.
    function _move(address from, address to, uint256 amount) internal {
        allowance[from][msg.sender] -= amount;
        _send(from, to, amount);
    }

    /// Reverts on a missing balance, by underflow.
    function _send(address from, address to, uint256 amount) internal virtual {
        balanceOf[from] -= amount;
        balanceOf[to] += amount;
    }
}

contract PlainToken is Ledger {
    function transfer(address to, uint256 amount) external returns (bool) {
        _send(msg.sender, to, amount);
        return true;
    }

    function transferFrom(address from, address to, uint256 amount) external returns (bool) {
        _move(from, to, amount);
        return true;
    }
}

/// Returns nothing from transfer and transferFrom, as some older tokens do.
contract SilentToken is Ledger {
    function transfer(address to, uint256 amount) external {
        _send(msg.sender, to, amount);
    }

    function transferFrom(address from, address to, uint256 amount) external {
        _move(from, to, amount);
    }
}

/// Moves nothing and returns false.
contract FalseToken is Ledger {
    function transfer(address, uint256) external pure returns (bool) {
        return false;
    }

    function transferFrom(address, address, uint256) external pure returns (bool) {
        return false;
    }
}
