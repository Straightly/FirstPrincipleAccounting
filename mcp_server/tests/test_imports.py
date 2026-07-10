"""M0 scaffold test: the package imports and contains no accounting engine/storage.

The Rust AccountingEngine owns domain invariants and storage (Impl Spec §3.4,
§7.1); the Python side is MCP + dev-time backend only.
"""

import importlib
import pathlib
import unittest

SRC = pathlib.Path(__file__).resolve().parents[1] / "src"


class TestPackageShape(unittest.TestCase):
    def test_package_imports(self):
        import sys

        sys.path.insert(0, str(SRC))
        for mod in (
            "first_principle_accounting",
            "first_principle_accounting.mcp",
            "first_principle_accounting.devtime",
            "first_principle_accounting.cli",
        ):
            importlib.import_module(mod)

    def test_no_engine_or_storage_modules(self):
        pkg = SRC / "first_principle_accounting"
        self.assertFalse((pkg / "engine").exists(), "engine belongs to the Rust crate")
        self.assertFalse((pkg / "storage").exists(), "storage belongs to the Rust crate")


if __name__ == "__main__":
    unittest.main()
