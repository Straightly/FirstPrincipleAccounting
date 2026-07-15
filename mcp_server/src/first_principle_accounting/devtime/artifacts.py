"""Artifact preparation: writes a generated workflow to the dev artifact
store (Impl Spec §7.4, §8.4).

Layout matches the hand-written M5 reference artifact exactly:
`<dev_artifacts_dir>/workflows/<workflow_deployment_id>/{workflow.json,
manifest.json, code/, signatures/}`. Hashes are computed by the *backend*
at deploy time (Impl Spec §7.4 — hashes are the identity authority, not
whatever the preparing process claims), so nothing here signs or hashes
anything; this module's only job is getting bytes onto disk in the right
shape, and leaving nothing behind if it fails partway through.
"""

from __future__ import annotations

import json
import pathlib
import shutil
import uuid

from ..config import Config
from .generator import GeneratedWorkflow

_CODE_FILES = ("react.production.min.js", "react-dom.production.min.js")


class ArtifactPreparationError(Exception):
    pass


def workflow_dir(config: Config, deployment_id: uuid.UUID) -> pathlib.Path:
    return config.dev_artifacts_dir / "workflows" / str(deployment_id)


def prepare_artifact(
    generated: GeneratedWorkflow, deployment_id: uuid.UUID, config: Config
) -> pathlib.Path:
    """Writes `generated` under `deployment_id`, substituting the deployment
    id into the code/manifest placeholders. Deployment ids are meant to be
    unique per deployment (Impl Spec §2.9) — refuses to overwrite an
    existing directory. On any failure, removes whatever was written so a
    retry never finds a half-written artifact.
    """
    target = workflow_dir(config, deployment_id)
    if target.exists():
        raise ArtifactPreparationError(
            f"artifact directory already exists: {target}"
        )

    deployment_id_str = str(deployment_id)
    try:
        code_dir = target / "code"
        signatures_dir = target / "signatures"
        code_dir.mkdir(parents=True)
        signatures_dir.mkdir()

        (target / "workflow.json").write_text(
            json.dumps(generated.workflow_json, indent=2) + "\n"
        )
        manifest = dict(generated.manifest_json)
        manifest["workflow_deployment_id"] = deployment_id_str
        (target / "manifest.json").write_text(
            json.dumps(manifest, indent=2) + "\n"
        )

        (code_dir / "index.html").write_text(generated.index_html)
        (code_dir / "app.js").write_text(
            generated.app_js.replace(
                "{{WORKFLOW_DEPLOYMENT_ID}}", deployment_id_str
            )
        )
        _vendor_react(code_dir, config)

        (signatures_dir / ".gitkeep").write_text("")
    except Exception as exc:
        shutil.rmtree(target, ignore_errors=True)
        raise ArtifactPreparationError(
            f"failed to prepare artifact {deployment_id}: {exc}"
        ) from exc

    return target


def _vendor_react(code_dir: pathlib.Path, config: Config) -> None:
    sources = {
        "react.production.min.js": config.react_vendor_dir
        / "react"
        / "umd"
        / "react.production.min.js",
        "react-dom.production.min.js": config.react_vendor_dir
        / "react-dom"
        / "umd"
        / "react-dom.production.min.js",
    }
    for filename, source in sources.items():
        if not source.is_file():
            raise ArtifactPreparationError(
                f"vendored React file not found: {source} — run `npm install` "
                "in frontend/ first"
            )
        shutil.copyfile(source, code_dir / filename)
