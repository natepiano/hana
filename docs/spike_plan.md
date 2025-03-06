# Spike Implementation

## Goal
Create minimal working system with:
- Local plugin loading and display
- MPE MIDI parameter mapping
- Error handling foundation
## Core Components Needed
1. visualization library (minimal)
    - Local visualization loading
    - Single window rendering
    - Basic parameter system

2. MPE Input
    - MIDI device connection
    - MPE message parsing
    - Parameter mapping

3. Error Handling
    - Using established error handling patterns
    - Focus on local operation errors

## Development Path
1. Create basic visualization loading (done)
2. Add window display (done)
3. Implement MPE input (not done)
4. Connect parameter system (not done)

# Visualization System - Initial Spike

## Visualization
A visualization must:
- Use the VisualizationControl plugin
- receive messages from the hana application visa the VisualizationControl bevy plugin
## Simple Test Plugin
The spike should include:
- A basic visualization
- Minimal parameter interface

## Visualization Loading
The application should:
- Load visualization from filesystem path
- Initialize visualization with window context
- Handle basic error cases (missing/invalid visualization)

## parameter mapping
- the hana app needs to connect to a local midi device and accept incoming MPE midi messages and send them to the VisualizationControl plugin for processing.
- right now i just want to focus on pressure coming from any key on the keyboard
- i want to map this in the examples/basic_visualization to moving all of the objects up and down on the vertical access
