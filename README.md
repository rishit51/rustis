# Key-Value Store Server

## Overview

This project implements a simple key-value store server (Redis) using Rust. It uses TCP/IP sockets for communication and handles basic commands for storing, retrieving, updating, and deleting key-value pairs.

## Features

- **Multi-threaded**: Handles multiple client connections concurrently using mio for event-driven I/O.
- **Command Handling**: Supports commands like `get`, `set`, `del`, and `keys`.
- **Error Handling**: Includes basic error handling for commands and network operations.
- **Custom Serialization**: Implements custom serialization for different types of responses (`NIL`, `ERR`, `STR`, `INT`, `ARR`).



## Installation

1. Clone the repository:

```
cargo build --release
```
Usage
Start the server:

```
cargo run --release
```
2. Connect clients to 127.0.0.1:8080 (default address and port).
  ```
rustc client.rs
./client <command> <args>

  ```
  

3. Use a TCP client to send commands (e.g., get, set, del, keys) to interact with the server.

## Configuration
1. Port: Default port is 8080. Modify main.rs to change the port.
2. Maximum Message Size: Configured via K_MAX_MSG in main.rs.
2. Maximum Arguments per Command: Configured via K_MAX_ARGS in main.rs.
## Contributing
Contributions are welcome! Please fork the repository and submit pull requests.

## License
This project is licensed under the MIT License - see the LICENSE file for details.



