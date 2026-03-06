// SPDX-License-Identifier: BSL-1.1
// GasHammer test contract — writes to N storage slots.
pragma solidity ^0.8.20;

contract StorageWriter {
    mapping(uint256 => uint256) public slots;
    uint256 public writeCount;

    /// Write sequential values to `count` storage slots.
    function writeSlots(uint256 count) external {
        for (uint256 i = 0; i < count; i++) {
            slots[writeCount + i] = block.timestamp + i;
        }
        writeCount += count;
    }

    /// Read a specific slot.
    function readSlot(uint256 index) external view returns (uint256) {
        return slots[index];
    }
}
