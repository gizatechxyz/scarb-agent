# Scarb Agent

Scarb Agent simplifies the creation of Cairo programs that interact seamlessly with constrained and unconstrained oracles, enabling developers to prove only the necessary components of their agents.

Scarb Agent is all you need to build provable agents ready for deployment on the [Giza](https://www.gizatech.xyz/) platform.

## ✨ Key Features:

- **Provable Cairo Programs**: Easily develop Cairo programs to prove the critical logic of your agent.
- **Custom Oracles**: Design and deploy both constrained and unconstrained oracles that operate concurrently with your Cairo programs.
- **Data Preprocessing and Postprocessing**: Manage data before and after the execution of Cairo programs.
- **Cross-chain Smart Contract Execution**: Enhance your Cairo programs with the capability to execute cross-chain smart contracts during its runtime.

##  Prerequisites

- Install `protoc` from [gRPC](https://grpc.io/docs/protoc-installation/).
- Download `scarb` from [Software Mansion's repository](https://github.com/software-mansion/scarb/releases).

## Installation

To install Scarb Agent, use the following command:

```bash
cargo install --git https://github.com/gizatechxyz/scarb-agent/
```

## Documentation 

Explore the [documentation](https://orion-giza.gitbook.io/scarb-agent) to learn how to get started with Scarb Agent.

## Starting a New Project

Initialize a new project using:

```bash
scarb agent-new [PROJECT_NAME]
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
