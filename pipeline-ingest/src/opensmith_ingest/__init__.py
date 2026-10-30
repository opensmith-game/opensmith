"""OpenSmith Pipeline-Ingest: songs → stems → transcription → playable game charts."""

__version__ = "0.1.0"


def main() -> None:
    from .cli import main as cli_main

    cli_main()
