// SPDX-License-Identifier: BSL-1.1
// GasHammer test contract — simple counter for state mutation testing.
pragma solidity ^0.8.20;

contract Counter {
    uint256 public count;

    function increment() external {
        count += 1;
    }

    function decrement() external {
        require(count > 0, "Counter: underflow");
        count -= 1;
    }

    function reset() external {
        count = 0;
    }

    function incrementBy(uint256 amount) external {
        count += amount;
    }
}
