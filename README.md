# Editron

Editron is an experimental media processing system and domain-specific language for building composable media pipelines.

The project focuses on explicit data representation, frame-level manipulation, and controlled execution.

---

## Project Goals

Editron is designed to:

1. Provide a simple, readable syntax for media workflows  
2. Maintain explicit control over data and execution  
3. Support performance-critical processing  
4. Handle multiple media formats through a unified interface  
5. Build a structured system for extensible media operations  

---

## Architecture Overview

Editron is composed of multiple subsystems:

### Lexer
Converts source code into tokens for further processing.

- Outputs `Vec<Token>`
- Implemented as a state machine
- Used as the first stage of the DSL pipeline

Details: [Lexer Architecture](docs/Lexer.md)

---

### Parser
Consumes tokens and emits intermediate representation (IR) with AST first design.

Details: [Parser](docs/Parser.md)

---

### Media Processing Engine

The execution core of Editron.

Responsible for:

- Decoding media
- Frame-level manipulation
- Audio processing
- Encoding output

Editron currently uses FFmpeg as a backend for encoding and decoding.  
FFmpeg must be installed and available in the system path.

---

## Core Data Types

Editron introduces several core data types:

### Frame
Represents a single image or video frame.

- Stores structured pixel data
- Supports multiple pixel formats
- Enables pixel-level manipulation

Details: [Frame](docs/Frame.md)

---

### Track
Represents decoded audio data.

- Stores normalized PCM samples
- Supports gain, mixing, and normalization

Details: [Track](docs/Track.md)

---



### Video
Provides access to video streams.

- Frame decoding
- Timestamp-based access
- Integration with FFmpeg backend

Details: [Video](docs/Video.md)

---

## Filter System

Filters are modular processing units applied to frames.

- Trait-based abstraction
- Extensible design
- Supports convolution-based operations

Details: [Filter System](docs/filters.md)

---

## DSL (Planned)

Editron is intended to support a domain-specific language for defining media pipelines.

Example syntax:

```edt
import "filters.edt" as filter;

frame1 = load("image.jpg")
frame2 = load("image2.png")

frame3 = frame1
    -> brightness(50)
    -> saturation(50)
    -> emboss()
    -> blend(frame2)
    -> filter::vivid()

export(frame3, "Outputs/image.png")