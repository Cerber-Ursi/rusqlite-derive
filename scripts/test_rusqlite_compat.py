"""Offline tests for the rusqlite release monitor and CI configuration."""

import contextlib
import io
import json
import pathlib
import tempfile
import unittest

import rusqlite_compat as compat

REQUIREMENT = ">=0.40.2, <0.44"
VERSIONS = ["0.40.2", "0.41.0", "0.41.1", "0.42.0", "0.43.0"]


def releases(*versions):
    return [{"num": version, "yanked": False} for version in versions]


class UpdatePlanTests(unittest.TestCase):
    def plan(self, *extra, current=VERSIONS, requirement=REQUIREMENT):
        return compat.plan_update(requirement, current, releases(*VERSIONS, *extra))

    def test_current_releases_are_unchanged(self):
        self.assertEqual(self.plan(), (REQUIREMENT, VERSIONS))

    def test_new_minor_widens_upper_bound_only(self):
        requirement, versions = self.plan("0.44.0")
        self.assertEqual(requirement, ">=0.40.2, <0.45")
        self.assertEqual(versions, [*VERSIONS, "0.44.0"])

    def test_all_new_minor_lines_are_added(self):
        requirement, versions = self.plan("0.44.0", "0.45.1")
        self.assertEqual(requirement, ">=0.40.2, <0.46")
        self.assertEqual(versions, [*VERSIONS, "0.44.0", "0.45.1"])
        self.assertNotIn("0.45.0", versions)

    def test_patch_keeps_first_and_latest_without_widening(self):
        requirement, versions = self.plan("0.43.1", "0.43.2")
        self.assertEqual(requirement, REQUIREMENT)
        self.assertEqual(versions, [*VERSIONS, "0.43.2"])
        self.assertNotIn("0.43.1", versions)

    def test_minimum_stays_in_matrix_after_a_patch(self):
        requirement, versions = self.plan("0.40.3")
        self.assertEqual(requirement, REQUIREMENT)
        self.assertEqual(versions[:2], ["0.40.2", "0.40.3"])

    def test_older_releases_do_not_lower_the_minimum(self):
        self.assertEqual(self.plan("0.39.0", "0.40.0"), (REQUIREMENT, VERSIONS))

    def test_upper_bound_never_shrinks(self):
        requirement, _ = self.plan(requirement=">=0.40.2, <0.45")
        self.assertEqual(requirement, ">=0.40.2, <0.45")

    def test_stable_major_release_uses_next_major_upper_bound(self):
        requirement, versions = self.plan("1.0.0")
        self.assertEqual(requirement, ">=0.40.2, <2.0")
        self.assertEqual(versions[-1], "1.0.0")

    def test_versions_are_sorted_numerically(self):
        requirement, versions = self.plan("0.99.0", "0.100.0")
        self.assertEqual(requirement, ">=0.40.2, <0.101")
        self.assertEqual(versions[-2:], ["0.99.0", "0.100.0"])

    def test_prereleases_and_yanked_releases_are_ignored(self):
        upstream = releases(*VERSIONS, "0.44.0-rc.1", "1.0.0-beta.1")
        upstream.append({"num": "0.44.0", "yanked": True})
        self.assertEqual(
            compat.plan_update(REQUIREMENT, VERSIONS, upstream),
            (REQUIREMENT, VERSIONS),
        )

    def test_previously_tested_release_is_retained_if_yanked(self):
        upstream = releases(*VERSIONS)
        upstream[1]["yanked"] = True
        self.assertEqual(
            compat.plan_update(REQUIREMENT, VERSIONS, upstream),
            (REQUIREMENT, VERSIONS),
        )

    def test_empty_supported_release_list_fails(self):
        with self.assertRaises(ValueError):
            compat.plan_update(REQUIREMENT, VERSIONS, releases("0.39.0", "0.44.0-rc.1"))


class ConfigurationTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = pathlib.Path(temporary.name)
        self.manifest = self.root / compat.MANIFEST
        self.matrix = self.root / compat.MATRIX
        self.manifest.parent.mkdir(parents=True)
        self.matrix.parent.mkdir(parents=True)
        self.manifest.write_text(
            '[dependencies]\n'
            f'rusqlite = {{ version = "{REQUIREMENT}", default-features = false }}\n'
            '[dev-dependencies]\n'
            f'rusqlite = {{ version = "{REQUIREMENT}", features = ["bundled"] }}\n'
        )
        self.matrix.write_text(json.dumps(VERSIONS, indent=2) + "\n")

    def test_load_configuration(self):
        self.assertEqual(compat.load_configuration(self.root), (REQUIREMENT, VERSIONS))

    def test_update_changes_both_bounds_and_the_matrix_idempotently(self):
        upstream = releases(*VERSIONS, "0.44.0")
        with contextlib.redirect_stdout(io.StringIO()):
            compat.update(self.root, upstream)
        manifest = self.manifest.read_text()
        matrix = self.matrix.read_text()
        self.assertEqual(manifest.count('version = ">=0.40.2, <0.45"'), 2)
        self.assertIn('default-features = false', manifest)
        self.assertIn('features = ["bundled"]', manifest)
        self.assertEqual(json.loads(matrix), [*VERSIONS, "0.44.0"])
        with contextlib.redirect_stdout(io.StringIO()):
            compat.update(self.root, upstream)
        self.assertEqual(self.manifest.read_text(), manifest)
        self.assertEqual(self.matrix.read_text(), matrix)

    def test_dependency_ranges_must_match(self):
        text = self.manifest.read_text().replace(REQUIREMENT, ">=0.40.2, <0.45", 1)
        self.manifest.write_text(text)
        with self.assertRaisesRegex(ValueError, "must match"):
            compat.load_configuration(self.root)

    def test_invalid_matrices_fail_before_ci_expansion(self):
        for versions in (
            [],
            {},
            ["0.41.0"],
            ["0.40.2", "0.44.0"],
            ["0.40.2", "0.41.0-rc.1"],
            ["0.40.2", "0.40.2"],
            ["0.41.0", "0.40.2"],
        ):
            with self.subTest(versions=versions):
                self.matrix.write_text(json.dumps(versions))
                with self.assertRaises(ValueError):
                    compat.load_configuration(self.root)

    def test_checked_in_matrix_covers_verified_releases(self):
        _, versions = compat.load_configuration(compat.ROOT)
        self.assertTrue({"0.40.2", "0.41.0", "0.42.0", "0.43.0"} <= set(versions))


if __name__ == "__main__":
    unittest.main()
