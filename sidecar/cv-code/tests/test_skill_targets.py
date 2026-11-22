"""Regression tests for the centered skill-target picker."""

import json
from pathlib import Path
import subprocess
import sys

import cv2
import numpy as np
import pytest

from mash_cv.cv import _find_skill_targets
from mash_cv.host.io import read_image


SCREENSHOTS = Path(__file__).parent / "test_data" / "screenshots"


@pytest.mark.parametrize(
    ("name", "expected"),
    [
        ("battle_select_one_cn.jpg", [0.499]),
        ("battle_select.png", [0.254, 0.499, 0.744]),
        ("battle.png", []),
    ],
)
def test_skill_targets_on_real_frames(name, expected):
    frame = read_image(str(SCREENSHOTS / name))
    assert frame is not None
    for image in (frame, cv2.resize(frame, (1920, 1080), interpolation=cv2.INTER_AREA)):
        assert _find_skill_targets(image) == [{"x": x, "y": 0.474} for x in expected]


def test_skill_target_picker_repl_returns_normalized_points():
    request = {
        "cmd": "find_skill_targets",
        "imagePath": str(SCREENSHOTS / "battle_select_one_cn.jpg"),
        "id": 1,
    }
    result = subprocess.run(
        [sys.executable, "-m", "mash_cv"],
        input=json.dumps(request) + "\n",
        text=True,
        capture_output=True,
        check=True,
        timeout=10,
    )
    assert json.loads(result.stdout) == {"id": 1, "targets": [{"x": 0.499, "y": 0.474}]}


def test_two_centered_targets_and_incomplete_layout():
    frame = np.zeros((1080, 1920, 3), dtype=np.uint8)
    source = read_image(str(SCREENSHOTS / "battle_select_one_cn.jpg"))
    assert source is not None
    source = cv2.resize(source, (1920, 1080), interpolation=cv2.INTER_AREA)
    portrait = source[400:530, 866:1050]
    frame[400:530, 631:815] = portrait
    assert _find_skill_targets(frame) == []
    frame[400:530, 1101:1285] = portrait
    assert _find_skill_targets(frame) == [{"x": x, "y": 0.474} for x in (0.3765, 0.6215)]
