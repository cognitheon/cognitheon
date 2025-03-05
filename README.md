# Cognitheon

A modern, interactive graph editor built with Rust and the egui framework, featuring GPU-accelerated particle effects and a flexible canvas system.

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

## Features

- **Interactive Node-Based Graph Editor**: Create, connect and manipulate nodes on a zoomable, pannable canvas.
- **Multiple Edge Types**: Support for different connection styles including Bezier curves and straight lines.
- **GPU-Accelerated Particle System**: Visually stunning effects rendered directly on the GPU.
- **Serialization Support**: Save and load your graph structures.
- **Multi-select and Manipulation**: Select multiple nodes and modify them simultaneously.
- **Customizable Appearance**: Adjust colors, node styles, and edge appearances.

## Architecture

Cognitheon is built using a modular architecture with the following components:

- **Core Graph Model**: Implemented with the petgraph library, providing a solid foundation for graph operations.
- **Canvas System**: Handles coordinate transformations between screen and canvas space, allowing for infinite zooming and panning.
- **Input Management**: A state-based input handling system that manages different interaction modes.
- **Rendering System**: Built on top of egui and wgpu for efficient 2D rendering and GPU-accelerated effects.
- **Resource Management**: Thread-safe shared resources using Arc and RwLock for state management.

## Getting Started

### Prerequisites

- Latest stable Rust toolchain
- Graphics drivers that support wgpu (most modern systems are compatible)

### Installation

1. Clone the repository:
   ```
   git clone https://github.com/yourusername/cognitheon.git
   cd cognitheon
   ```

2. Build and run the application:
   ```
   cargo run --release
   ```

## Usage

### Basic Operations

- **Create Nodes**: Right-click on empty canvas space
- **Connect Nodes**: Drag from one node to another
- **Delete Elements**: Select and press Delete
- **Select Multiple**: Drag selection rectangle or Ctrl+Click
- **Pan Canvas**: Middle mouse button or Alt+Left drag
- **Zoom**: Mouse wheel

### File Operations

- **New Project**: File > New
- **Save Project**: File > Save
- **Load Project**: File > Open

### Edge Types

Switch between different edge types through the Edge Type dropdown in the UI panel:
- Bezier curves: Smooth, adjustable paths with control points
- Straight lines: Direct connections between nodes

## Web Deployment

Cognitheon can be compiled to WebAssembly and deployed as a web application:

1. Install the WASM target:
   ```
   rustup target add wasm32-unknown-unknown
   ```

2. Install Trunk:
   ```
   cargo install --locked trunk
   ```

3. Build for web:
   ```
   trunk build --release
   ```

4. The generated `dist` directory can be deployed to any static web hosting service.

## Development

### Project Structure

- `src/app.rs`: Main application structure and UI layout
- `src/graph/`: Graph data structures and operations
- `src/canvas.rs`: Canvas system for coordinate management
- `src/ui/`: UI components and rendering
- `src/input/`: Input handling and state management
- `src/gpu_render/`: GPU-accelerated rendering components

### Building with Debugging

```
cargo run
```

## Contributing

We welcome contributions to Cognitheon! Please feel free to submit a Pull Request.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add some amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## License

This project is dual-licensed under either:

- MIT License ([LICENSE-MIT](LICENSE-MIT))
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))

at your option.

## Acknowledgments

- Built on [egui](https://github.com/emilk/egui) and [eframe](https://github.com/emilk/egui/tree/master/crates/eframe)
- Graph algorithms powered by [petgraph](https://github.com/petgraph/petgraph)
- GPU rendering with [wgpu](https://github.com/gfx-rs/wgpu)
