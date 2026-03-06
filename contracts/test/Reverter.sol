// SPDX-License-Identifier: BSL-1.1
// GasHammer test contract — conditionally reverts for testing revert handling.
pragma solidity ^0.8.20;

contract Reverter {
    uint256 public callCount;

    /// Reverts if condition is true, otherwise increments counter.
    function revertIf(bool condition) external {
        if (condition) {
            revert("Reverter: intentional revert");
        }
        callCount += 1;
    }

    /// Always reverts with a custom message.
    function alwaysRevert() external pure {
        revert("Reverter: always reverts");
    }

    /// Reverts with no message (out of gas simulation).
    function revertEmpty() external pure {
        revert();
    }
}
