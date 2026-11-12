"""Read only the current CN servant level, excluding the slash and level cap."""

from __future__ import annotations

from typing import Callable
import math
import unicodedata

import cv2
import numpy as np


# The automation stream is 1920×1080. Normalized bounds also support offline
# higher-resolution captures without changing the recognition crop's geometry.
LEVEL_REGION = {"x": 634 / 1920, "y": 670 / 1080, "w": 259 / 1920, "h": 86 / 1080}


def split_current_level(image: np.ndarray, region: dict | None = None) -> tuple[np.ndarray | None, dict]:
    """Locate the thin rising slash in the image, then retain whole digits left of it.

    No text recognition or level-cap parsing participates in this separation.
    Unknown/ambiguous geometry returns no crop instead of guessing a prefix.
    """
    diagnostics: dict = {"failReason": None}

    def fail(reason: str):
        diagnostics["failReason"] = reason
        return None, diagnostics

    if image.size == 0:
        return fail("empty_frame")
    height, width = image.shape[:2]
    bounds = region or LEVEL_REGION
    x0 = max(0, round(bounds["x"] * width))
    y0 = max(0, round(bounds["y"] * height))
    x1 = min(width, x0 + round(bounds["w"] * width))
    y1 = min(height, y0 + round(bounds["h"] * height))
    if x1 <= x0 or y1 <= y0:
        return fail("empty_region")
    # Canonical pixel geometry keeps component/slash checks independent of the
    # source frame size. Only this small field is resized, not the full frame.
    roi = cv2.resize(image[y0:y1, x0:x1], (259, 86), interpolation=cv2.INTER_AREA)
    gray = cv2.cvtColor(roi, cv2.COLOR_BGR2GRAY)
    mask = cv2.inRange(gray, 235, 255)
    count, labels, stats, _ = cv2.connectedComponentsWithStats(mask, 8)
    components = []
    slashes = []
    for index in range(1, count):
        x, y, w, h, area = (int(value) for value in stats[index])
        if not (28 <= h <= 60 and 5 <= w <= 40 and 40 <= area <= 900):
            continue
        yy, xx = np.where(labels[y:y + h, x:x + w] == index)
        slope, intercept = np.polyfit(yy, xx, 1)
        residual = float(np.sqrt(np.mean((xx - slope * yy - intercept) ** 2)))
        component = {"x": x, "y": y, "w": w, "h": h, "area": area}
        components.append(component)
        # A slash is a thin, almost straight negative-slope stroke; a narrow
        # digit such as 1/7 alone does not satisfy both slope and residual.
        if 0.18 <= w / h <= 0.43 and -0.5 <= slope <= -0.12 and residual <= 1.5:
            slashes.append(component)
    diagnostics["components"] = components
    if len(slashes) != 1:
        return fail("slash_not_found" if not slashes else "ambiguous_slash")
    slash = slashes[0]
    diagnostics["slash"] = slash
    digits = [item for item in components if item["x"] + item["w"] < slash["x"]]
    digits.sort(key=lambda item: item["x"])
    if not 1 <= len(digits) <= 3:
        return fail("current_digits_not_found")
    for item in digits:
        overlap = min(item["y"] + item["h"], slash["y"] + slash["h"]) - max(item["y"], slash["y"])
        if overlap < min(item["h"], slash["h"]) * 0.7:
            return fail("current_digits_misaligned")
    if slash["x"] - (digits[-1]["x"] + digits[-1]["w"]) > 35:
        return fail("current_digits_too_far")
    if any(right["x"] - (left["x"] + left["w"]) > 24 for left, right in zip(digits, digits[1:])):
        return fail("current_digits_disconnected")
    left = min(item["x"] for item in digits)
    top = min(item["y"] for item in digits)
    right = max(item["x"] + item["w"] for item in digits)
    bottom = max(item["y"] + item["h"] for item in digits)
    if left <= 1 or top <= 1 or bottom >= mask.shape[0] - 1:
        return fail("current_digits_clipped")
    # Copy only the accepted glyph bodies. The pale panel, slash, and digits on
    # its right cannot leak into the recognition-only OCR model.
    digit_mask = np.zeros((bottom - top, right - left), dtype=np.uint8)
    for item in digits:
        x, y, w, h = (item[key] for key in ("x", "y", "w", "h"))
        digit_mask[y - top:y - top + h, x - left:x - left + w] = mask[y:y + h, x:x + w]
    crop = cv2.cvtColor(255 - digit_mask, cv2.COLOR_GRAY2BGR)
    crop = cv2.copyMakeBorder(crop, 10, 10, 10, 10, cv2.BORDER_CONSTANT, value=(255, 255, 255))
    diagnostics["digitCount"] = len(digits)
    diagnostics["currentBounds"] = {"x": left, "y": top, "w": right - left, "h": bottom - top}
    return crop, diagnostics


def read_current_level(image: np.ndarray, recognize: Callable, region: dict | None = None) -> dict:
    crop, diagnostics = split_current_level(image, region)
    result = {"found": False, "current": None, "text": "", "failReason": diagnostics["failReason"], "diagnostics": diagnostics}
    if crop is None:
        return result
    raw, _ = recognize([crop])
    if len(raw) != 1:
        result["failReason"] = diagnostics["failReason"] = "current_ocr_empty"
        return result
    text, confidence = raw[0]
    text = unicodedata.normalize("NFKC", str(text)).strip()
    result["text"] = text
    diagnostics["ocrConfidence"] = float(confidence)
    if not text.isascii() or not text.isdigit() or len(text) != diagnostics["digitCount"] or text.startswith("0"):
        result["failReason"] = "current_ocr_not_complete_digits"
    elif not math.isfinite(float(confidence)) or float(confidence) < 0.95:
        result["failReason"] = "current_ocr_low_confidence"
    elif not 1 <= int(text) <= 150:
        result["failReason"] = "current_level_out_of_range"
    else:
        result.update(found=True, current=int(text), failReason=None)
    diagnostics["failReason"] = result["failReason"]
    return result
