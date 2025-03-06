# Documentation Guidelines
{{#include ../ai.md}}
## Purpose
This document outlines Hana's documentation guidelines, defining what should be documented, where documentation should live, and how to maintain it effectively.

## Documentation Categories

### Always Document
1. **Crate Public APIs and Interfaces**
    - All public API endpoints and their parameters
    - Visualization SDK
    - Network protocols and message formats
    - Configuration file formats and options

2. **System Architecture**
    - High-level system components and their interactions
    - Core subsystem responsibilities
    - Critical data flows
    - Security model and trust boundaries

3. **User-Facing Features**
    - Installation and setup procedures
    - Basic usage instructions
    - Common troubleshooting steps
    - Visualization development guides
    - Configuration guides

4. **Critical Decisions**
    - Major architectural choices and their rationale
    - Technology selection justifications
    - Security model decisions
    - Performance trade-offs

### Document Selectively
1. **Internal Implementation**
    - Document complex algorithms or non-obvious solutions
    - Skip obvious implementations or standard patterns
    - Focus on "why" rather than "what" for internal code

2. **Development Processes**
    - Document unusual or project-specific practices
    - Skip standard Git workflows or common development patterns
    - Focus on exceptions rather than rules

3. **Testing**
    - Document test strategy and framework choices
    - Skip individual test case documentation unless complex
    - Focus on test patterns and shared utilities

### Skip Documentation
1. **Standard Patterns**
    - Common design patterns
    - Basic Rust idioms
    - Standard library usage
    - Common error handling patterns

2. **Temporary Code**
    - Development scaffolding
    - Test fixtures
    - Debug logging
    - Prototype implementations

3. **Self-Documenting Code**
    - Clear function names and signatures
    - Simple data structures
    - Standard implementations
    - Obvious control flow

## Documentation Locations

### Code Documentation
1. **Public API Documentation**
    - Location: Module-level documentation
    - Style: Full Rust doc comments
    - Include: Examples, errors, edge cases

2. **Internal Documentation**
    - Location: Function/type level comments
    - Style: Brief explanatory comments
    - Focus: Non-obvious decisions or complex logic

3. **Implementation Notes**
    - Location: Inline comments
    - Style: Concise, focused on "why"
    - Use: Sparingly, only where needed

## Documentation Style

### General Guidelines
- Use clear, concise language
- Focus on practical examples
- Include context and rationale
- Keep formatting consistent

### Code Comments
- Explain "why" not "what"
- Reference issue numbers where relevant
- Document assumptions and edge cases
- Keep comments up to date

### Markdown Standards
- Use consistent headers
- Include table of contents for long docs
- Use code blocks with language tags
- Maintain consistent formatting
