//! Arbitrary template-file matching operations.

use super::*;

impl SidecarClient {
    /// Match one UI template (by sidecar template key) inside a small region.
    /// Designed for 10-30 ms polling loops: the sidecar caches the scaled
    /// template and only runs a single `matchTemplate` on the region crop.
    pub fn probe_template(
        &mut self,
        image_path: Option<&Path>,
        template_key: &str,
        region: NormRect,
        threshold: f64,
        template_reference_width: u32,
    ) -> Result<ProbeTemplateResult, CvError> {
        let mut req = request(
            SidecarCommand::ProbeTemplate,
            serde_json::json!({
                "templateKey": template_key,
                "templateReferenceWidth": template_reference_width,
                "region": {
                    "x": region.x,
                    "y": region.y,
                    "w": region.w,
                    "h": region.h,
                },
                "threshold": threshold,
            }),
        )?;
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|value| value.as_str()) {
            return Err(format!("probe_template({template_key}): {err}").into());
        }
        Ok(serde_json::from_value::<ProbeTemplateResult>(resp)
            .map_err(|error| format!("invalid probe_template response: {error}"))?)
    }

    /// Recognition-only batch over small text strips (no detector pass).
    /// Used by the enhancement runner to synthesize just the text it needs
    /// instead of a full-frame OCR.
    pub fn recognize_strips(
        &mut self,
        image_path: Option<&Path>,
        regions: &[NormRect],
    ) -> Result<Vec<String>, CvError> {
        let mut req = request(
            SidecarCommand::RecognizeStrips,
            serde_json::json!({
                "regions": regions
                    .iter()
                    .map(|region| serde_json::json!({
                        "x": region.x,
                        "y": region.y,
                        "w": region.w,
                        "h": region.h,
                    }))
                    .collect::<Vec<_>>(),
            }),
        )?;
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|value| value.as_str()) {
            return Err(format!("recognize_strips: {err}").into());
        }
        let texts = resp
            .get("texts")
            .and_then(|value| value.as_array())
            .map(|items| {
                items
                    .iter()
                    .map(|item| item.as_str().unwrap_or("").to_string())
                    .collect()
            })
            .unwrap_or_default();
        Ok(texts)
    }

    /// Search for an arbitrary grayscale template file within a region.
    #[allow(dead_code)]
    pub fn find_region(
        &mut self,
        image_path: Option<&Path>,
        template_path: &Path,
        region: NormRect,
        threshold: f64,
    ) -> Result<Option<Point>, CvError> {
        let mut req = request(
            SidecarCommand::FindRegion,
            serde_json::json!({
                "templatePath": template_path.to_string_lossy(),
                "region": {
                    "x": region.x,
                    "y": region.y,
                    "w": region.w,
                    "h": region.h,
                },
                "threshold": threshold,
            }),
        )?;
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|value| value.as_str()) {
            return Err(format!("find_region({}): {err}", template_path.display()).into());
        }
        if resp["found"].as_bool().unwrap_or(false) {
            let x = resp["x"].as_f64().unwrap_or(0.0);
            let y = resp["y"].as_f64().unwrap_or(0.0);
            Ok(Some(Point::new(x, y)))
        } else {
            Ok(None)
        }
    }

    /// Resize an outfit template to one fixed pixel size and match it once.
    /// The sidecar returns the score even below `threshold` for diagnostics.
    pub fn find_region_with_template_size(
        &mut self,
        image_path: Option<&Path>,
        template_path: &Path,
        region: NormRect,
        threshold: f64,
        template_size: (u32, u32),
    ) -> Result<ElementMatch, CvError> {
        let mut req = request(
            SidecarCommand::FindRegion,
            serde_json::json!({
                "templatePath": template_path.to_string_lossy(),
                "region": {
                    "x": region.x,
                    "y": region.y,
                    "w": region.w,
                    "h": region.h,
                },
                "threshold": threshold,
                "templateSize": { "w": template_size.0, "h": template_size.1 },
                "alphaMask": true,
                "alphaBackground": 224,
            }),
        )?;
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|value| value.as_str()) {
            return Err(format!(
                "find_region_with_template_size({}): {err}",
                template_path.display()
            )
            .into());
        }
        let found = resp["found"].as_bool().unwrap_or(false);
        let score = resp["score"].as_f64().unwrap_or(0.0);
        let x = resp["x"].as_f64().unwrap_or(0.0);
        let y = resp["y"].as_f64().unwrap_or(0.0);
        let region = resp.get("region").and_then(|region| {
            Some(NormRect {
                x: region.get("x")?.as_f64()?,
                y: region.get("y")?.as_f64()?,
                w: region.get("w")?.as_f64()?,
                h: region.get("h")?.as_f64()?,
            })
        });
        Ok(ElementMatch {
            found,
            x,
            y,
            score,
            region,
        })
    }

    /// Search for an arbitrary template file after cropping the template.
    #[allow(dead_code)]
    pub fn find_region_with_template_crop(
        &mut self,
        image_path: Option<&Path>,
        template_path: &Path,
        region: NormRect,
        template_crop: NormRect,
        template_size: Option<(u32, u32)>,
        threshold: f64,
    ) -> Result<Option<Point>, CvError> {
        let mut req = request(
            SidecarCommand::FindRegion,
            serde_json::json!({
                "templatePath": template_path.to_string_lossy(),
                "region": {
                    "x": region.x,
                    "y": region.y,
                    "w": region.w,
                    "h": region.h,
                },
                "templateCrop": {
                    "x": template_crop.x,
                    "y": template_crop.y,
                    "w": template_crop.w,
                    "h": template_crop.h,
                },
                "threshold": threshold,
            }),
        )?;
        if let Some((width, height)) = template_size {
            if let Some(object) = req.as_object_mut() {
                object.insert(
                    "templateSize".into(),
                    serde_json::json!({ "w": width, "h": height }),
                );
            }
        }
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|value| value.as_str()) {
            return Err(format!("find_region({}): {err}", template_path.display()).into());
        }
        if resp["found"].as_bool().unwrap_or(false) {
            let x = resp["x"].as_f64().unwrap_or(0.0);
            let y = resp["y"].as_f64().unwrap_or(0.0);
            Ok(Some(Point::new(x, y)))
        } else {
            Ok(None)
        }
    }

    /// Search for an arbitrary template file after cropping the template,
    /// returning the raw match payload even when it misses the threshold.
    #[allow(dead_code)]
    pub fn find_region_with_template_crop_full(
        &mut self,
        image_path: Option<&Path>,
        template_path: &Path,
        region: NormRect,
        template_crop: NormRect,
        template_size: Option<(u32, u32)>,
        threshold: f64,
    ) -> Result<ElementMatch, CvError> {
        let mut req = request(
            SidecarCommand::FindRegion,
            serde_json::json!({
                "templatePath": template_path.to_string_lossy(),
                "region": {
                    "x": region.x,
                    "y": region.y,
                    "w": region.w,
                    "h": region.h,
                },
                "templateCrop": {
                    "x": template_crop.x,
                    "y": template_crop.y,
                    "w": template_crop.w,
                    "h": template_crop.h,
                },
                "threshold": threshold,
            }),
        )?;
        if let Some((width, height)) = template_size {
            if let Some(object) = req.as_object_mut() {
                object.insert(
                    "templateSize".into(),
                    serde_json::json!({ "w": width, "h": height }),
                );
            }
        }
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|value| value.as_str()) {
            return Err(format!("find_region({}): {err}", template_path.display()).into());
        }
        let found = resp["found"].as_bool().unwrap_or(false);
        let score = resp["score"].as_f64().unwrap_or(0.0);
        let x = resp["x"].as_f64().unwrap_or(0.0);
        let y = resp["y"].as_f64().unwrap_or(0.0);
        let region = resp.get("region").and_then(|region| {
            Some(NormRect {
                x: region.get("x")?.as_f64()?,
                y: region.get("y")?.as_f64()?,
                w: region.get("w")?.as_f64()?,
                h: region.get("h")?.as_f64()?,
            })
        });
        Ok(ElementMatch {
            found,
            x,
            y,
            score,
            region,
        })
    }
}
