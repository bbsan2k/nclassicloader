#!/usr/bin/env python3
"""Mechanical regression checks for the build-tree Apple nall adaptation."""
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
HELPER = ROOT / "portable/core/apple_compat.cmake"


class AppleCompatTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.cmake = shutil.which("cmake")
        if cls.cmake is None:
            raise unittest.SkipTest("CMake is required for Apple compatibility tests")
        (ROOT / ".build").mkdir(exist_ok=True)

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="apple-compat-", dir=ROOT / ".build")
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.upstream = self.base / "synthetic upstream with spaces"
        self.output = self.base / "explicit build output with spaces"
        nall = self.upstream / "nall/nall"
        nall.mkdir(parents=True)
        self.memory = nall / "memory.hpp"
        self.runtime = nall / "nall.cpp"
        self.memory.write_bytes(b"#pragma once\n#if defined(PLATFORM_MACOS)\nint memory;\n#endif\n")
        self.runtime.write_bytes(b"#include <nall/one.cpp>\n#include <nall/run.cpp>\n#include <nall/two.cpp>\n")

    def invoke(self):
        script = self.base / "invoke.cmake"
        script.write_text(
            f'include("{HELPER.as_posix()}")\n'
            'clover_prepare_apple_nall("${ARES_ROOT}" "${OUTPUT_DIR}" GENERATED_SOURCE)\n'
            'file(WRITE "${RESULT_FILE}" "${GENERATED_SOURCE}")\n'
        )
        return subprocess.run(
            [self.cmake, f"-DARES_ROOT={self.upstream}", f"-DOUTPUT_DIR={self.output}",
             f"-DRESULT_FILE={self.base / 'result.txt'}", "-P", str(script)],
            capture_output=True, text=True,
        )

    def test_transforms_only_requested_patterns_without_touching_fixture(self):
        original_memory = self.memory.read_bytes()
        original_runtime = self.runtime.read_bytes()
        result = self.invoke()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        generated_memory = (self.output / "nall/memory.hpp").read_text()
        self.assertEqual(generated_memory.count("#if defined(PLATFORM_MACOS) && TARGET_OS_OSX"), 1)
        self.assertIn("#include <TargetConditionals.h>\n#pragma once", generated_memory)
        self.assertNotIn("#if defined(PLATFORM_MACOS)\n", generated_memory)
        generated_runtime = (self.output / "nall-runtime.cpp").read_text()
        self.assertEqual(generated_runtime.count("#include <nall/run.cpp>"), 0)
        self.assertIn("#include <nall/one.cpp>", generated_runtime)
        self.assertIn("#include <nall/two.cpp>", generated_runtime)
        self.assertEqual(self.memory.read_bytes(), original_memory)
        self.assertEqual(self.runtime.read_bytes(), original_runtime)
        self.assertEqual(Path((self.base / "result.txt").read_text()).resolve(), (self.output / "nall-runtime.cpp").resolve())
        self.assertTrue((self.output / "nall-runtime.cpp").is_file())

    def test_invalid_pattern_counts_and_missing_inputs_fail(self):
        cases = (
            (b"#pragma once\n", self.runtime.read_bytes(), False, False),
            (b"#if defined(PLATFORM_MACOS)\n#if defined(PLATFORM_MACOS)\n", self.runtime.read_bytes(), False, False),
            (self.memory.read_bytes(), None, False, True),
            (self.memory.read_bytes(), b"#include <nall/one.cpp>\n", False, False),
            (self.memory.read_bytes(), b"#include <nall/run.cpp>\n#include <nall/run.cpp>\n", False, False),
        )
        for memory, runtime, missing_memory, missing_runtime in cases:
            with self.subTest(memory=memory, runtime=runtime):
                if missing_memory:
                    self.memory.unlink(missing_ok=True)
                else:
                    self.memory.write_bytes(memory)
                if missing_runtime:
                    self.runtime.unlink(missing_ok=True)
                else:
                    self.runtime.write_bytes(runtime)
                result = self.invoke()
                self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertFalse((self.output / "nall-runtime.cpp").exists())
                self.memory.write_bytes(b"#pragma once\n#if defined(PLATFORM_MACOS)\nint memory;\n#endif\n")
                self.runtime.write_bytes(b"#include <nall/one.cpp>\n#include <nall/run.cpp>\n#include <nall/two.cpp>\n")


if __name__ == "__main__":
    unittest.main()
