"""CN current-level reader: actual glyph crops, independent of full-field OCR."""

import json
from pathlib import Path
import subprocess
import sys

import cv2
import numpy as np
import pytest

from mash_cv.enhancement_level import read_current_level, split_current_level


DATA = Path(__file__).with_name("test_data") / "enhancement-level-cn"
SOURCE = Path(__file__).resolve().parents[1]


def fixture(level):
    image = cv2.imread(str(DATA / f"current_{level}.png"))
    assert image is not None
    return image


def frame(crop, width=1920):
    image = np.zeros((1080, 1920, 3), dtype=np.uint8)
    image[670:756, 634:893] = crop
    return cv2.resize(image, (width, width * 9 // 16), interpolation=cv2.INTER_CUBIC)


def recognizer(text, confidence=1.0):
    return lambda images: ([(text, confidence)], 0.0)


@pytest.mark.parametrize("level", (1, 37, 40, 80))
@pytest.mark.parametrize("width", (1920, 2560))
def test_splits_actual_current_digits_before_recognition(level, width):
    crop, diagnostics = split_current_level(frame(fixture(level), width))
    assert diagnostics["failReason"] is None
    assert diagnostics["digitCount"] == len(str(level))
    bounds = diagnostics["currentBounds"]
    assert bounds["x"] + bounds["w"] < diagnostics["slash"]["x"]
    assert crop is not None
    assert crop.shape[1] == bounds["w"] + 20
    assert np.all(crop[:10] == 255)


def test_excludes_maximum_even_if_its_text_changes():
    image = fixture(37)
    original, _ = split_current_level(frame(image))
    image[:, 145:] = 0
    changed, diagnostics = split_current_level(frame(image))
    assert diagnostics["digitCount"] == 2
    np.testing.assert_array_equal(original, changed)


@pytest.mark.parametrize("kind", ("blank", "missing_slash", "missing_current", "duplicate_slash", "clipped"))
def test_ambiguous_or_incomplete_field_never_reaches_ocr(kind):
    image = fixture(37)
    if kind == "blank":
        image[:] = 0
    elif kind == "missing_slash":
        image[:, 120:141] = 0
    elif kind == "missing_current":
        image[:, :120] = 0
    elif kind == "duplicate_slash":
        image[26:68, 220:232] = image[26:68, 124:136]
    elif kind == "clipped":
        image = np.roll(image, 50, axis=0)
        image[:50] = 0

    def unexpected(images):
        pytest.fail("Invalid geometry must be rejected before OCR")

    result = read_current_level(frame(image), unexpected)
    assert result["found"] is False
    assert result["current"] is None
    assert result["failReason"]


@pytest.mark.parametrize("text,confidence", (("371", 1.0), ("3/7", 1.0), ("37 40", 1.0), ("0", 1.0), ("37", 0.8), ("37", float("nan")), ("", 1.0)))
def test_rejects_non_numeric_low_confidence_or_wrong_digit_count(text, confidence):
    result = read_current_level(frame(fixture(37)), recognizer(text, confidence))
    assert result["found"] is False
    assert result["current"] is None


def test_does_not_truncate_371_to_37():
    result = read_current_level(frame(fixture(37)), recognizer("371"))
    assert result["failReason"] == "current_ocr_not_complete_digits"


def test_fullwidth_digits_are_normalized():
    assert read_current_level(frame(fixture(37)), recognizer("３７"))["current"] == 37


def composite_level(text):
    """Compose 3-digit boundary cases from the real game font, not another font."""
    sources = {
        "1": (fixture(1), (90, 24, 14, 41)),
        "3": (fixture(37), (55, 24, 27, 40)),
        "7": (fixture(37), (89, 25, 25, 41)),
        "4": (fixture(40), (57, 24, 26, 41)),
        "0": (fixture(40), (88, 24, 27, 40)),
        "8": (fixture(80), (56, 25, 26, 40)),
    }
    image = fixture(37)
    image[:, :120] = 0
    right = 115
    for digit in reversed(text):
        source, (x, y, w, h) = sources[digit]
        right -= w
        image[24:24 + h, right:right + w] = source[y:y + h, x:x + w]
        right -= 5
    return image


@pytest.mark.parametrize("text,expected", (("100", 100), ("140", 140), ("180", None)))
def test_three_digits_and_current_range(text, expected):
    result = read_current_level(frame(composite_level(text)), recognizer(text))
    assert result["current"] == expected
    assert result["found"] is (expected is not None)


@pytest.fixture(scope="module")
def real_recognizer():
    from mash_cv import cv
    cv._set_server("CN")
    yield cv._get_ocr().text_recognizer
    cv._release_ocr()


@pytest.mark.parametrize("level", (1, 37, 40, 80, 100, 140))
@pytest.mark.parametrize("width", (1920, 2560))
def test_actual_recognition_only_model(real_recognizer, level, width):
    image = fixture(level) if level < 100 else composite_level(str(level))
    result = read_current_level(frame(image, width), real_recognizer)
    assert result["found"] is True, result
    assert result["current"] == level


def test_repl_current_level_command_with_request_id(tmp_path):
    image_path = tmp_path / "level-37.png"
    assert cv2.imwrite(str(image_path), frame(fixture(37)))
    commands = [
        {"cmd": "set_server", "server": "CN", "id": 10},
        {"cmd": "read_enhancement_current_level", "imagePath": str(image_path), "id": 11},
        {"cmd": "quit"},
    ]
    result = subprocess.run(
        [sys.executable, "-m", "mash_cv"], cwd=SOURCE,
        input="\n".join(json.dumps(command) for command in commands) + "\n",
        capture_output=True, text=True, timeout=30,
    )
    assert result.returncode == 0, result.stderr
    responses = [json.loads(line) for line in result.stdout.splitlines() if line]
    response = next(item for item in responses if item.get("id") == 11)
    assert response["found"] is True, response
    assert response["current"] == 37
    assert "max" not in response
