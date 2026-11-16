import importlib.util
from pathlib import Path
import unittest

spec=importlib.util.spec_from_file_location("native_supervisor",Path(__file__).resolve().parents[1]/"scripts/test-cef-native.py")
module=importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

class NativeSupervisorTests(unittest.TestCase):
    def test_gio_critical_is_a_runtime_failure(self):
        line="(process:42): GLib-GIO-CRITICAL: invalid connection"
        self.assertEqual(module.runtime_criticals("startup\n"+line+"\n"),[line])
    def test_unrelated_diagnostics_are_not_classified_as_gio_critical(self):
        self.assertEqual(module.runtime_criticals("Gtk-WARNING: allocation\nERROR: dbus system bus unavailable"),[])
