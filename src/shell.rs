use std::process::Child;

#[allow(dead_code)]
pub struct Job {
    pub id: usize,
    pub command: String,
    pub children: Vec<Child>,
}

#[allow(dead_code)]
#[derive(Default)]
pub struct Shell {
    pub history: Vec<String>,
    pub sync_idx: usize,
    pub jobs: Vec<Job>,
}

#[allow(dead_code)]
impl Shell {
    pub fn next_job_id(&self) -> usize {
        self.jobs.iter().map(|j| j.id).max().unwrap_or(0) + 1
    }
}
