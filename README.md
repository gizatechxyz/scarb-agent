# Scarb Agent

Scarb Agent is all you need to build provable agents ready for deployment on the [Giza](https://www.gizatech.xyz/) platform.
Prove only what you need to prove! Scarb Agent makes it easy to implement Cairo programs that can interact with custom oracles.

**Key Features:**

- Preprocess and postprocess data surrounding the execution of Cairo programs.
- Execute cross-chain smart contracts during your Cairo program runtime.
- Design and deploy both constrained and unconstrained custom oracles.

## Prerequisites

- Install `protoc` from [gRPC](https://grpc.io/docs/protoc-installation/).
- Download `scarb` from [Software Mansion's repository](https://github.com/software-mansion/scarb/releases).

## Installation

To install Scarb Agent, use the following command:

```bash
cargo install --git https://github.com/gizatechxyz/scarb-agent/
```

## Starting a New Project

Initialize a new project using:

```bash
scarb agent-new [PROJECT_NAME]
```

After creation, run the following commands:

```bash
scarb agent-generate
scarb build
```

## Usage

1. Start the agent server:

   ```
   cd python
   python3 -m venv .venv
   source .venv/bin/activate
   pip install -r requirements.txt
   python src/main.py
   ```

2. In the root of your project, run the agent:
   ```
   scarb agent-run --args-json [ARGS_CAIRO_FUNCTION]
   ```

## Preprocessing

To run preprocessing:

1. Ensure the Python server is running.
2. Use the `--preprocess` flag when running the Scarb agent:
   ```
   scarb agent-run --preprocess --args-json '{"n": 9}'
   ```

## Postprocessing

To run postprocessing:

1. Ensure the Python server is running.
2. Use the `--postprocess` flag when running the Scarb agent:
   ```
   scarb agent-run --postprocess --args-json '{"n": 9}'
   ```

## Acknowledgments

This project builds upon the implementation of [Cairo-Hints](https://github.com/reilabs/cairo-hints) by Reilabs. Special thanks to [Reilabs](https://reilabs.io/) for their contributions to the Cairo ecosystem.
