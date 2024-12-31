# Documentation Guidelines

## Overview
This document outlines Hana's documentation guidelines, defining what should be documented, where documentation should live, and how to maintain it effectively.

## Documentation Categories

### Always Document
1. **Public APIs and Interfaces**
    - All public API endpoints and their parameters
    - Plugin interfaces and lifecycle hooks
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
    - Plugin development guides
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

### Documentation Structure

```
docs/
├── architecture/          # System design docs
│   ├── overview.md        # High-level system description
│   ├── network.md         # Network architecture
│   └── security.md        # Security model
├── developer/             # Developer guides
│   ├── building.md        # Build instructions
│   ├── testing.md         # Testing guidelines
│   └── plugins/           # Plugin development guides
├── user/                  # User documentation
│   ├── installation.md    # Installation guide
│   ├── configuration.md   # Configuration guide
│   └── tutorials/         # Usage tutorials
└── README.md              # Project overview

src/
└── {module}/              # Source code 
    └── mod.rs             # Module docs generate API reference

target/
└── doc/                   # Generated API documentation
```

### Documentation Types

1. **User Documentation** (`/docs/user/`)
    - Installation and setup
    - Configuration guides
    - Usage tutorials
    - Troubleshooting

2. **Architecture Documentation** (`/docs/architecture/`)
    - System design docs
    - Component interactions
    - Design decisions
    - Security model

3. **Developer Documentation** (`/docs/developer/`)
    - Build instructions
    - Testing guidelines
    - Plugin development
    - Contributing guide

4. **API Documentation** (Generated)
    - Auto-generated from source comments
    - Built using `cargo doc`
    - Published with releases
    - Includes module, type, and function docs

## Documentation Maintenance

### Review Process
- Documentation review in PR process
- Regular documentation audits
- User feedback incorporation
- Documentation testing (examples, links)

### Update Triggers
1. **Must Update**
    - API changes
    - Feature additions/removals
    - Security model changes
    - Configuration changes

2. **Consider Updating**
    - Implementation improvements
    - Performance optimizations
    - Internal refactoring
    - Test strategy changes

3. **Skip Updates**
    - Minor bug fixes
    - Regular maintenance
    - Style changes
    - Temporary changes

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

## Implementation Notes

This documentation strategy aims to balance completeness with maintainability. It prioritizes user-facing documentation and critical system understanding while avoiding documentation overhead for standard or obvious components.

The strategy should evolve with the project, adapting to user feedback and development needs while maintaining its core focus on essential documentation.

## Doc Links
- [Architecture](../architecture/README.md) - High level system design
- [Developer](../developer/README.md) - Coding guidelines for hana contributors
- [Overview](../../README.md) - Hana overview
- [Plugin Development](../plugins/README.md) - Guidelines for plugin development
- [User](../developer/README.md) - Hana user documentation
