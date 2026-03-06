// SPDX-License-Identifier: BSL-1.1
// GasHammer test contract — burns a specified amount of gas via computation.
pragma solidity ^0.8.20;

contract GasBurner {
    uint256 public lastBurned;

    /// Burns approximately `gasAmount` gas via a compute loop.
    function burn(uint256 gasAmount) external {
        uint256 startGas = gasleft();
        uint256 x = 1;
        while (startGas - gasleft() < gasAmount) {
            x = uint256(keccak256(abi.encodePacked(x)));
        }
        lastBurned = startGas - gasleft();
    }
}
