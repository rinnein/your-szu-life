//! UI challenge ownership. Cancellation only takes this short-lived mutex, never the operation/OCR lock.
use super::{
    captcha,
    client::Client,
    protocol::{Error, Failure, Result},
    Service,
};
use base64::Engine;
use serde_json::{json, Value};
#[derive(Clone)]
struct Challenge {
    id: String,
    client: Client,
    token: String,
    bytes: Vec<u8>,
}
#[derive(Default)]
pub(super) struct Flow {
    revision: u64,
    challenge: Option<Challenge>,
}
impl Flow {
    fn advance(&mut self, revision: u64) -> bool {
        if revision < self.revision {
            return false;
        }
        self.revision = revision;
        true
    }
    fn take(&mut self, revision: u64, id: &str) -> Option<Challenge> {
        if self.revision != revision || self.challenge.as_ref()?.id != id {
            return None;
        }
        self.challenge.take()
    }
}
fn cancelled() -> Value {
    json!({"status":"cancelled"})
}
fn valid_points(points: &[[u32; 2]]) -> bool {
    points.len() <= 4
        && points
            .iter()
            .enumerate()
            .all(|(i, p)| p[0] < 250 && p[1] < 80 && !points[..i].contains(p))
}
impl Service {
    pub(super) fn cancel_captcha(&self, flow: &str, revision: u64, discard: bool) -> Result<Value> {
        let mut flows = self.captcha_flows.lock().unwrap();
        let state = flows.entry(flow.into()).or_default();
        if state.advance(revision) && discard {
            state.challenge = None;
        }
        Ok(cancelled())
    }
    fn valid_captcha_client(&self, c: &Client) -> bool {
        let state = self.snapshot();
        state.mode == c.mode
            && state.context.student == c.student
            && self
                .credentials
                .load_profile()
                .ok()
                .flatten()
                .is_some_and(|p| p.student_id == c.student)
            && self
                .shared_client(c.mode)
                .ok()
                .is_some_and(|s| s.generation == c.generation)
    }
    pub(super) fn prepare_captcha(&self, flow: &str, revision: u64) -> Result<Value> {
        {
            let mut flows = self.captcha_flows.lock().unwrap();
            let state = flows.entry(flow.into()).or_default();
            if !state.advance(revision) {
                return Ok(cancelled());
            }
            state.challenge = None;
        }
        let c = {
            let _operation = self.operation.lock().unwrap();
            let profile = self.identity()?;
            let existing = self.client(false)?;
            if !existing.token.is_empty() {
                if let Err(e) = self.refresh_context(&existing, &profile.student_id) {
                    if e.kind != Failure::Session {
                        return Err(e);
                    }
                }
                if self.snapshot().logged_in {
                    return Ok(json!({"status":"authenticated","snapshot":self.snapshot()}));
                }
            }
            self.client(true)?
        };
        let (token, bytes) = c.captcha()?;
        if !self.valid_captcha_client(&c) {
            return Ok(cancelled());
        }
        let mut flows = self.captcha_flows.lock().unwrap();
        let state = flows.entry(flow.into()).or_default();
        if state.revision != revision {
            return Ok(cancelled());
        }
        let id = uuid::Uuid::new_v4().to_string();
        let image = format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(&bytes)
        );
        state.challenge = Some(Challenge {
            id: id.clone(),
            client: c,
            token,
            bytes,
        });
        Ok(json!({"status":"challenge","challengeId":id,"image":image,"revision":revision}))
    }
    pub(super) fn captcha_point(
        &self,
        flow: &str,
        revision: u64,
        id: &str,
        points: &[[u32; 2]],
    ) -> Result<Value> {
        if !valid_points(points) || points.len() >= 4 {
            return Err(Error::new(Failure::Protocol, "验证码点位无效"));
        }
        let challenge = {
            let mut flows = self.captcha_flows.lock().unwrap();
            let state = flows.entry(flow.into()).or_default();
            if !state.advance(revision) {
                return Ok(cancelled());
            }
            let Some(c) = state.challenge.as_ref().filter(|c| c.id == id) else {
                return Ok(cancelled());
            };
            c.clone()
        };
        if !self.valid_captcha_client(&challenge.client) {
            return Ok(cancelled());
        }
        let result = captcha::recognize_next(&challenge.bytes, points);
        if !self.valid_captcha_client(&challenge.client) {
            return Ok(cancelled());
        }
        let flows = self.captcha_flows.lock().unwrap();
        if flows.get(flow).is_none_or(|s| {
            s.revision != revision || s.challenge.as_ref().is_none_or(|c| c.id != id)
        }) {
            return Ok(cancelled());
        }
        Ok(match result {
            Ok(point) => {
                json!({"status":"point","challengeId":id,"revision":revision,"index":points.len(),"point":point})
            }
            Err(e) => {
                json!({"status":match e {
                    captcha::RecognitionError::Structure => "structural",
                    captcha::RecognitionError::ModelMissing => "modelMissing",
                    captcha::RecognitionError::Uncertain => "uncertain",
                }})
            }
        })
    }
    pub(super) fn submit_captcha(
        &self,
        flow: &str,
        revision: u64,
        id: &str,
        points: &[[u32; 2]],
    ) -> Result<Value> {
        captcha::coordinates(points).map_err(Error::from)?;
        let mut challenge = {
            let mut flows = self.captcha_flows.lock().unwrap();
            let state = flows.entry(flow.into()).or_default();
            if !state.advance(revision) {
                return Ok(cancelled());
            }
            let Some(c) = state.take(revision, id) else {
                return Ok(cancelled());
            };
            c
        };
        let _operation = self.operation.lock().unwrap();
        if !self.valid_captcha_client(&challenge.client) {
            return Ok(cancelled());
        }
        let profile = self
            .credentials
            .load_profile()
            .map_err(Error::from)?
            .ok_or_else(|| Error::new(Failure::Session, "账户已退出"))?;
        match challenge.client.login(
            &profile.student_id,
            &profile.password,
            &challenge.token,
            points,
        ) {
            Ok(()) => {
                self.save_client(&challenge.client)?;
                self.refresh_context(&challenge.client, &profile.student_id)?;
                self.resume_authenticated()?;
                Ok(json!({"status":"authenticated","snapshot":self.snapshot()}))
            }
            Err(e)
                if e.kind == Failure::Business
                    && ["验证码", "校验码", "verifyCode", "vcode"]
                        .iter()
                        .any(|s| e.message.contains(s)) =>
            {
                Ok(json!({"status":"rejected","message":e.message}))
            }
            Err(e) => Err(e),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancellation_rejects_old_revisions() {
        let mut f = Flow::default();
        assert!(f.advance(2));
        assert!(!f.advance(1));
        assert_eq!(f.revision, 2);
        assert!(f.advance(3));
    }
    #[test]
    fn challenge_is_consumed_once_and_wrong_ids_do_not_consume_it() {
        let client = Client::from_jar(
            crate::enrollment::model::Mode::Campus,
            String::new(),
            crate::enrollment::session::jar("").unwrap(),
            "synthetic".into(),
        )
        .unwrap();
        let mut f = Flow {
            revision: 1,
            challenge: Some(Challenge {
                id: "c".into(),
                client,
                token: "synthetic".into(),
                bytes: vec![],
            }),
        };
        assert!(f.take(0, "c").is_none());
        assert!(f.take(1, "other").is_none());
        assert!(f.take(1, "c").is_some());
        assert!(f.take(1, "c").is_none());
    }
    #[test]
    fn validates_partial_points_without_duplicate_or_out_of_bounds() {
        assert!(valid_points(&[[10, 30], [30, 30]]));
        assert!(!valid_points(&[[10, 30], [10, 30]]));
        assert!(!valid_points(&[[250, 0]]));
    }
}
