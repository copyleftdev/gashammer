// SPDX-License-Identifier: BSL-1.1
// GasHammer test contract — shared state for testing contention scenarios.
pragma solidity ^0.8.20;

contract ContentionTarget {
    uint256 public value;
    mapping(address => uint256) public contributions;

    /// Atomically increment value. Generates contention when multiple
    /// accounts call this simultaneously.
    function contribute(uint256 amount) external {
        value += amount;
        contributions[msg.sender] += amount;
    }

    /// Atomic swap: set value to newValue, return old value.
    function swap(uint256 newValue) external returns (uint256 oldValue) {
        oldValue = value;
        value = newValue;
    }
}
