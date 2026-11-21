use super::*;

impl SidecarClient {
    pub fn find_interlude_dialogue_choices(&mut self) -> Result<Vec<Point>, CvError> {
        let req = request(SidecarCommand::FindInterludeDialogueChoices, serde_json::json!({}))?;
        let mut resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|value| value.as_str()) {
            return Err(CvError::Other(err.to_string()));
        }
        serde_json::from_value(resp["choices"].take())
            .map_err(|error| format!("invalid find_interlude_dialogue_choices response: {error}").into())
    }

    pub fn find_interlude_quest_rows(
        &mut self,
        image_path: Option<&Path>,
    ) -> Result<FindInterludeQuestRowsResult, CvError> {
        let mut req = request(
            SidecarCommand::FindInterludeQuestRows,
            serde_json::json!({}),
        )?;
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|value| value.as_str()) {
            return Err(CvError::Other(err.to_string()));
        }
        Ok(serde_json::from_value::<FindInterludeQuestRowsResult>(resp)
            .map_err(|error| format!("invalid find_interlude_quest_rows response: {error}"))?)
    }
}
