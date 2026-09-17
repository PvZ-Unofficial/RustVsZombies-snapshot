import tempfile
import unittest
from pathlib import Path

from check_lto import NEEDLE, new_build_dir, verify_ir


class EvidenceTests(unittest.TestCase):
    def test_stale_build_directory_is_rejected_even_if_empty(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaises(FileExistsError):
                new_build_dir(Path(directory))
            fresh = Path(directory) / "fresh"
            self.assertEqual(new_build_dir(fresh), fresh.resolve())

    def test_requires_both_removed_call_and_imported_native_store(self):
        before = f"call void @{NEEDLE}()"
        after = "store i8 1, ptr @gModifiers"
        self.assertEqual(verify_ir(before, after)["calls_before"], 1)
        self.assertIsNone(verify_ir("unrelated module", after))
        for bad in [before + "\n" + after, "@gModifiers = global i8 0", "unrelated module"]:
            with self.assertRaises(ValueError):
                verify_ir(before, bad)
        with self.assertRaises(ValueError):
            verify_ir(before + "\n" + after, after)


if __name__ == "__main__":
    unittest.main()
