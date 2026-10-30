"""Commands from plugins: run when installed, and the files they import listed for the app."""

import json

from opensmith_ingest import cli


def register(sub, machine):
    p = sub.add_parser("example")
    p.add_argument("file")
    p.add_argument("--progress-json", action="store_true", help=machine)
    return lambda args, emit: print(f"plugin got {args.file}")


register.formats = (".EX", "exa")


class Entry:
    name = "example"

    @staticmethod
    def load():
        return register


def test_an_installed_plugin_adds_its_command(monkeypatch, capsys):
    monkeypatch.setattr(cli, "entry_points", lambda group: [Entry] if group == cli.PLUGINS else [])
    cli.main(["example", "song.ex"])
    assert capsys.readouterr().out.strip() == "plugin got song.ex"


def test_the_app_learns_which_files_plugins_import(monkeypatch, capsys):
    monkeypatch.setattr(cli, "entry_points", lambda group: [Entry] if group == cli.PLUGINS else [])
    cli.main(["formats"])
    assert json.loads(capsys.readouterr().out) == {"ex": "example", "exa": "example"}
