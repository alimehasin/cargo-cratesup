# Cratesup

A Cargo subcommand that scans your Cargo.toml dependencies and checks for available updates to help you keep your dependencies up-to-date.

https://github.com/user-attachments/assets/c25f448d-e0b3-4836-a714-6260c1dd6588

## Installation

```bash
cargo install cargo-cratesup
```

## Usage

Once installed, you can use Cratesup with the following commands:

```bash
cargo cratesup
```

This command will display a list of available updates for your dependencies without modifying your Cargo.toml file.

```bash
cargo cratesup -u
```

Add the `--update` flag or `-u` to automatically update your Cargo.toml file with the latest available versions.

## Features

- Check for updates in your Rust project's dependencies
- Update your Cargo.toml file with the latest available versions

## Contributing

Contributions are welcome! If you find a bug or have a feature request, please open an issue or submit a pull request.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

<a href="https://github.com/alimehasin/cargo-cratesup/graphs/contributors">
  <img src="https://opencollective.com/cargo-cratesup/contributors.svg?width=890&button=false">
</a>
