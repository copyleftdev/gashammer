// SPDX-License-Identifier: BSL-1.1
// GasHammer test contract — emits events for testing event-based verification.
pragma solidity ^0.8.20;

contract EventEmitter {
    event TestEvent(bytes32 indexed topic, bytes data);
    event Ping(address indexed sender, uint256 timestamp);

    uint256 public emitCount;

    /// Emit a test event with arbitrary topic and data.
    function emit_(bytes32 topic, bytes calldata data) external {
        emit TestEvent(topic, data);
        emitCount += 1;
    }

    /// Emit a simple ping event.
    function ping() external {
        emit Ping(msg.sender, block.timestamp);
        emitCount += 1;
    }
}
