"""Regression tests for the skill-slot border-row detector."""

import numpy as np

from mash_cv.cv import _find_skill_slots


POINTS = [
    {"x": x, "y": 0.807}
    for xs in ([0.058, 0.127, 0.196], [0.305, 0.374, 0.443], [0.553, 0.622, 0.691])
    for x in xs
]


def _frame_with_borders(missing: set[int]) -> np.ndarray:
    height, width = 1080, 1920
    img = np.zeros((height, width, 3), dtype=np.uint8)
    border_y = int(0.761 * height)
    for index, point in enumerate(POINTS):
        if index in missing:
            continue
        x0 = int((point["x"] - 0.023) * width)
        x1 = int((point["x"] + 0.023) * width)
        img[border_y : border_y + 2, x0:x1] = 255
    return img


def test_border_rows_mark_every_present_skill():
    result = _find_skill_slots(_frame_with_borders(set()), POINTS)
    assert [slot["index"] for slot in result["slots"] if slot["present"]] == list(
        range(9)
    )
    assert all(slot["score"] == 1.0 for slot in result["slots"])


def test_missing_border_is_reported_as_absent():
    result = _find_skill_slots(_frame_with_borders({5}), POINTS)
    present = [slot["index"] for slot in result["slots"] if slot["present"]]
    assert present == [0, 1, 2, 3, 4, 6, 7, 8]
    assert result["slots"][5]["present"] is False
    assert result["slots"][5]["score"] < 0.5
