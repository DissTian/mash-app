//! Enhancement-grid and Craft Essence recognition operations.

use super::*;

impl SidecarClient {
    /// Read only the current servant level from the fixed CN enhancement ROI.
    pub fn read_enhancement_current_level(
        &mut self,
        image_path: Option<&Path>,
    ) -> Result<EnhancementCurrentLevelResult, CvError> {
        let mut req = request(SidecarCommand::ReadEnhancementCurrentLevel, serde_json::json!({}))?;
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv_with_timeout(&req, RECOVERABLE_OCR_COMMAND_TIMEOUT)?;
        if let Some(err) = resp.get("error").and_then(|value| value.as_str()) {
            return Err(CvError::Other(err.to_string()));
        }
        Ok(serde_json::from_value::<EnhancementCurrentLevelResult>(resp)
            .map_err(|error| format!("invalid read_enhancement_current_level response: {error}"))?)
    }

    pub fn find_enhancement_servant_grid(
        &mut self,
        image_path: Option<&Path>,
        face_template_paths: &[PathBuf],
        region: NormRect,
        template_crop: NormRect,
        template_size: Option<(u32, u32)>,
        threshold: f64,
        retry_seconds: f64,
        mask_region: Option<NormRect>,
        direct_search_threshold: Option<f64>,
        collect_timings: bool,
        skip_grid: bool,
    ) -> Result<FindEnhancementServantGridResult, CvError> {
        let mut req = request(
            SidecarCommand::FindEnhancementServantGrid,
            serde_json::json!({
                "anchorTemplateKey": "text_servant_avatar_bottom_line",
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
                "faceThreshold": threshold,
                "retrySeconds": retry_seconds,
                "retryIntervalSeconds": 0.15,
                "faceTemplatePaths": face_template_paths
                    .iter()
                    .map(|p| p.to_string_lossy().into_owned())
                    .collect::<Vec<_>>(),
            }),
        )?;
        if let Some((w, h)) = template_size {
            if let Some(obj) = req.as_object_mut() {
                obj.insert("templateSize".into(), serde_json::json!({ "w": w, "h": h }));
            }
        }
        if let Some(region) = mask_region {
            if let Some(obj) = req.as_object_mut() {
                obj.insert(
                    "maskRegion".into(),
                    serde_json::json!({
                        "x": region.x,
                        "y": region.y,
                        "w": region.w,
                        "h": region.h,
                    }),
                );
            }
        }
        if let Some(threshold) = direct_search_threshold {
            if let Some(obj) = req.as_object_mut() {
                obj.insert("directSearchThreshold".into(), serde_json::json!(threshold));
            }
        }
        if collect_timings {
            if let Some(obj) = req.as_object_mut() {
                obj.insert("timings".into(), serde_json::json!(true));
            }
        }
        if skip_grid {
            if let Some(obj) = req.as_object_mut() {
                obj.insert("skipGrid".into(), serde_json::json!(true));
            }
        }
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            return Err(CvError::Other(err.to_string()));
        }
        Ok(
            serde_json::from_value::<FindEnhancementServantGridResult>(resp)
                .map_err(|e| format!("invalid find_enhancement_servant_grid response: {e}"))?,
        )
    }

    #[allow(dead_code)] // Kept for compatibility with the generic item-grid CV command.
    pub fn find_item_grid(
        &mut self,
        image_path: Option<&Path>,
        anchor_template_key: &str,
        anchor_template_reference_width: f64,
        region: NormRect,
        retry_seconds: f64,
    ) -> Result<FindItemGridResult, CvError> {
        let mut req = request(
            SidecarCommand::FindItemGrid,
            serde_json::json!({
                "anchorTemplateKey": anchor_template_key,
                "anchorTemplateReferenceWidth": anchor_template_reference_width,
                "region": {
                    "x": region.x,
                    "y": region.y,
                    "w": region.w,
                    "h": region.h,
                },
                "retrySeconds": retry_seconds,
                "retryIntervalSeconds": 0.15,
            }),
        )?;
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            return Err(CvError::Other(err.to_string()));
        }
        Ok(serde_json::from_value::<FindItemGridResult>(resp)
            .map_err(|e| format!("invalid find_item_grid response: {e}"))?)
    }

    pub fn read_craft_essence_grid(
        &mut self,
        image_path: Option<&Path>,
        anchor_template_key: &str,
        anchor_template_reference_width: f64,
        region: NormRect,
        retry_seconds: f64,
    ) -> Result<ReadCraftEssenceGridResult, CvError> {
        let mut req = request(
            SidecarCommand::ReadCraftEssenceGrid,
            serde_json::json!({
                "anchorTemplateKey": anchor_template_key,
                "anchorTemplateReferenceWidth": anchor_template_reference_width,
                "region": {
                    "x": region.x,
                    "y": region.y,
                    "w": region.w,
                    "h": region.h,
                },
                "retrySeconds": retry_seconds,
                "retryIntervalSeconds": 0.15,
            }),
        )?;
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            return Err(CvError::Other(err.to_string()));
        }
        Ok(serde_json::from_value::<ReadCraftEssenceGridResult>(resp)
            .map_err(|e| format!("invalid read_craft_essence_grid response: {e}"))?)
    }

    pub fn read_craft_essence_main_target(
        &mut self,
        image_path: Option<&Path>,
    ) -> Result<ReadCraftEssenceMainTargetResult, CvError> {
        let mut req = request(
            SidecarCommand::ReadCraftEssenceMainTarget,
            serde_json::json!({}),
        )?;
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            return Err(CvError::Other(err.to_string()));
        }
        Ok(
            serde_json::from_value::<ReadCraftEssenceMainTargetResult>(resp)
                .map_err(|e| format!("invalid read_craft_essence_main_target response: {e}"))?,
        )
    }
}
