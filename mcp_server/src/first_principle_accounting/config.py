"""Configuration for the MCP server and dev-time backend (Impl Spec §7.1).

Every value here is either the Rust backend's HTTP base URL or a local
filesystem path — never an accounting storage credential or a book
encryption key (Axiom 12). Accounting context this process ever touches is
fetched transiently through the runtime backend's authenticated HTTP API,
exactly like a browser would, never by opening a book file directly.
"""

from __future__ import annotations

import dataclasses
import os
import pathlib


def _repo_root() -> pathlib.Path:
    # mcp_server/src/first_principle_accounting/config.py -> repo root
    return pathlib.Path(__file__).resolve().parents[3]


@dataclasses.dataclass(frozen=True)
class Config:
    backend_base_url: str = dataclasses.field(
        default_factory=lambda: os.environ.get(
            "LZ_MCP_BACKEND_URL", "http://127.0.0.1:8080"
        )
    )
    # The developer is the sole deploy authority in v1 (Impl Spec §6.2); this
    # dev-time process authenticates to the backend as that one identity via
    # the same dev-login/OAuth session flow the browser uses.
    dev_login_email: str = dataclasses.field(
        default_factory=lambda: os.environ.get(
            "LZ_MCP_DEV_LOGIN_EMAIL", "zhian.job@gmail.com"
        )
    )
    dev_artifacts_dir: pathlib.Path = dataclasses.field(
        default_factory=lambda: pathlib.Path(
            os.environ.get(
                "LZ_MCP_DEV_ARTIFACTS_DIR", str(_repo_root() / "dev_artifacts")
            )
        )
    )
    # Source of the vendored React/ReactDOM UMD builds generated artifacts
    # ship with (Impl Spec §7.1: each workflow bundle carries its own React
    # copy, no shared JS dependencies between workflows or the launcher).
    react_vendor_dir: pathlib.Path = dataclasses.field(
        default_factory=lambda: pathlib.Path(
            os.environ.get(
                "LZ_MCP_REACT_VENDOR_DIR",
                str(_repo_root() / "frontend" / "node_modules"),
            )
        )
    )


def load_config() -> Config:
    return Config()
