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

    /// Polls all jobs, returns formatted lines, and removes finished jobs.
    /// Markers are computed on the full list before removal.
    /// With `include_running = false`, only `Done` lines are returned.
    pub fn job_lines(&mut self, include_running: bool) -> Vec<String> {
        let done: Vec<bool> = self
            .jobs
            .iter_mut()
            .map(|job| {
                job.children
                    .iter_mut()
                    .all(|c| matches!(c.try_wait(), Ok(Some(_)) | Err(_)))
            })
            .collect();

        let n = self.jobs.len();
        let mut lines = Vec::new();
        for (i, job) in self.jobs.iter().enumerate() {
            let marker = if i + 1 == n {
                '+'
            } else if i + 2 == n {
                '-'
            } else {
                ' '
            };
            if done[i] {
                lines.push(format!(
                    "[{}]{}  {:<24}{}",
                    job.id, marker, "Done", job.command
                ));
            } else if include_running {
                lines.push(format!(
                    "[{}]{}  {:<24}{} &",
                    job.id, marker, "Running", job.command
                ));
            }
        }

        let mut i = 0;
        self.jobs.retain(|_| {
            let keep = !done[i];
            i += 1;
            keep
        });
        lines
    }
}
