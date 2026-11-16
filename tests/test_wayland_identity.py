import importlib.util
from pathlib import Path
import unittest
import signal

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('wayland_identity', ROOT/'scripts/test-wayland-identity.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class WaylandTraceTests(unittest.TestCase):
    def test_termination_unwinds_cleanup(self):
        cleaned = []
        with self.assertRaises(SystemExit) as error:
            try:
                module.terminate(signal.SIGTERM, None)
            finally:
                cleaned.append(True)
        self.assertEqual(error.exception.code, 143)
        self.assertEqual(cleaned, [True])

    def test_identity_override_is_tracked_per_toplevel(self):
        trace = '''[1.0] -> xdg_toplevel@27.set_app_id("site.blig.Paisley.p6162")
[1.1] -> xdg_toplevel@27.set_app_id("site.blig.Paisley")
[1.2] -> xdg_toplevel#38.set_app_id("site.blig.Paisley")
'''
        self.assertEqual(module.last_app_ids(trace), {'27': module.DESKTOP_ID, '38': module.DESKTOP_ID})

    def test_forwarding_client_has_no_toplevel(self):
        self.assertEqual(module.last_app_ids('wl_surface@23.commit()'), {})

    def test_unfixed_profile_ids_are_not_normalized_away(self):
        trace = 'xdg_toplevel@27.set_app_id("site.blig.Paisley.p64656661756c74.Private")'
        self.assertNotEqual(set(module.last_app_ids(trace).values()), {module.DESKTOP_ID})

    def test_identity_is_canonical_before_first_rendered_buffer(self):
        trace = '''xdg_wm_base@18.get_xdg_surface(new id xdg_surface@26, wl_surface@23)
xdg_surface@26.get_toplevel(new id xdg_toplevel@27)
xdg_toplevel@27.set_app_id("profile.id")
xdg_toplevel@27.set_app_id("site.blig.Paisley")
wl_surface@23.attach(wl_buffer@29, 0, 0)
'''
        self.assertEqual(module.first_painted_id(trace), module.DESKTOP_ID)
        late = trace.replace('xdg_toplevel@27.set_app_id("site.blig.Paisley")\n', '') + 'xdg_toplevel@27.set_app_id("site.blig.Paisley")'
        self.assertEqual(module.first_painted_id(late), 'profile.id')
