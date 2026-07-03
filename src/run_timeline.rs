use std::time::{Duration, Instant};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RunTimelineStatus {
    Running,
    WaitingApproval,
    Succeeded,
    Failed,
    Cancelled,
}

impl RunTimelineStatus {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Running => "выполняется",
            Self::WaitingApproval => "ждёт доступа",
            Self::Succeeded => "готово",
            Self::Failed => "ошибка",
            Self::Cancelled => "отменено",
        }
    }
}

#[derive(Clone, Debug)]
pub struct RunTimelineStep {
    pub id: String,
    pub title: String,
    pub detail: String,
    pub status: RunTimelineStatus,
    pub started_at: Option<Instant>,
    pub finished_after: Option<Duration>,
    pub output: String,
}

impl RunTimelineStep {
    fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        detail: impl Into<String>,
        status: RunTimelineStatus,
    ) -> Self {
        let started_at = if matches!(
            status,
            RunTimelineStatus::Running | RunTimelineStatus::WaitingApproval
        ) {
            Some(Instant::now())
        } else {
            None
        };
        Self {
            id: id.into(),
            title: title.into(),
            detail: detail.into(),
            status,
            started_at,
            finished_after: None,
            output: String::new(),
        }
    }

    pub fn duration_label(&self) -> Option<String> {
        let duration = self
            .finished_after
            .or_else(|| self.started_at.map(|started| started.elapsed()))?;
        Some(format_duration(duration))
    }
}

#[derive(Clone, Debug)]
pub struct RunTimeline {
    pub title: String,
    pub steps: Vec<RunTimelineStep>,
    pub started_at: Instant,
    pub finished_at: Option<Instant>,
    pub final_report: Option<String>,
    pub changed_files: Vec<String>,
    pub failed: bool,
}

impl RunTimeline {
    pub fn new(user_message: &str) -> Self {
        let mut timeline = Self {
            title: compact(user_message, 90),
            steps: Vec::new(),
            started_at: Instant::now(),
            finished_at: None,
            final_report: None,
            changed_files: Vec::new(),
            failed: false,
        };
        timeline.steps.push(RunTimelineStep::new(
            "planning",
            "Планирование",
            "Агент получил задачу и готовит первый шаг.",
            RunTimelineStatus::Running,
        ));
        timeline
    }

    pub fn mark_assistant_text(&mut self) {
        if let Some(step) = self.steps.iter_mut().find(|step| step.id == "planning") {
            if matches!(step.status, RunTimelineStatus::Running) {
                finish_step(
                    step,
                    RunTimelineStatus::Succeeded,
                    "План принят моделью; начат ответ.",
                );
            }
        }
    }

    pub fn tool_started(&mut self, id: String, name: String, summary: String) {
        if let Some(step) = self.steps.iter_mut().find(|step| step.id == "planning") {
            if matches!(step.status, RunTimelineStatus::Running) {
                finish_step(
                    step,
                    RunTimelineStatus::Succeeded,
                    "План принят; начаты инструментальные действия.",
                );
            }
        }
        let action = extract_act_action(&summary).unwrap_or(name.as_str());
        let title = if name == "act" {
            format!("Инструмент: {action}")
        } else {
            format!("Инструмент: {name}")
        };
        self.steps.push(RunTimelineStep::new(
            id,
            title,
            summary,
            RunTimelineStatus::Running,
        ));
    }

    pub fn tool_output(&mut self, id: &str, chunk: &str) {
        if let Some(step) = self.steps.iter_mut().find(|step| step.id == id) {
            append_compact(&mut step.output, chunk, 2_400);
        }
    }

    pub fn tool_finished(&mut self, id: &str, output: &str) {
        if let Some(step) = self.steps.iter_mut().find(|step| step.id == id) {
            append_compact(&mut step.output, output, 2_400);
            finish_step(step, RunTimelineStatus::Succeeded, "Инструмент завершён.");
        }
    }

    pub fn approval_requested(&mut self, id: String, summary: String, detail: String) {
        self.steps.push(RunTimelineStep::new(
            id,
            format!("Согласование: {}", compact(&summary, 80)),
            detail,
            RunTimelineStatus::WaitingApproval,
        ));
    }

    pub fn approval_answered(&mut self, id: &str, approved: bool) {
        if let Some(step) = self.steps.iter_mut().find(|step| step.id == id) {
            finish_step(
                step,
                if approved {
                    RunTimelineStatus::Succeeded
                } else {
                    RunTimelineStatus::Cancelled
                },
                if approved {
                    "Доступ согласован."
                } else {
                    "Доступ отклонён."
                },
            );
        }
    }

    pub fn fail(&mut self, error: &str) {
        self.failed = true;
        if let Some(step) = self.steps.iter_mut().rev().find(|step| {
            matches!(
                step.status,
                RunTimelineStatus::Running | RunTimelineStatus::WaitingApproval
            )
        }) {
            finish_step(step, RunTimelineStatus::Failed, error);
        } else {
            self.steps.push(RunTimelineStep::new(
                "error",
                "Ошибка",
                error,
                RunTimelineStatus::Failed,
            ));
        }
    }

    pub fn cancel_requested(&mut self) {
        if let Some(step) = self.steps.iter_mut().rev().find(|step| {
            matches!(
                step.status,
                RunTimelineStatus::Running | RunTimelineStatus::WaitingApproval
            )
        }) {
            finish_step(
                step,
                RunTimelineStatus::Cancelled,
                "Пользователь запросил остановку.",
            );
        }
    }

    pub fn finish(&mut self, changed_files: &[String]) {
        let finished = Instant::now();
        self.finished_at = Some(finished);
        self.changed_files = changed_files.to_vec();
        for step in &mut self.steps {
            if matches!(step.status, RunTimelineStatus::Running) {
                let status = if self.failed {
                    RunTimelineStatus::Failed
                } else {
                    RunTimelineStatus::Succeeded
                };
                finish_step(step, status, "Шаг завершён вместе с запуском.");
            }
        }
        self.final_report = Some(self.build_report(changed_files));
    }

    pub fn elapsed_label(&self) -> String {
        let end = self.finished_at.unwrap_or_else(Instant::now);
        format_duration(end.saturating_duration_since(self.started_at))
    }

    pub fn executed_commands(&self) -> Vec<String> {
        self.steps
            .iter()
            .filter(|step| is_command_step(step))
            .map(|step| compact(&step.detail, 140))
            .collect()
    }

    pub fn validation_steps(&self) -> Vec<String> {
        self.steps
            .iter()
            .filter(|step| is_validation_step(step))
            .map(|step| format!("{} — {}", step.title, step.status.label()))
            .collect()
    }

    fn build_report(&self, changed_files: &[String]) -> String {
        let status = if self.failed {
            "завершён с ошибкой"
        } else {
            "завершён"
        };
        let changed = if changed_files.is_empty() {
            vec!["нет данных о текущих изменённых файлах".to_string()]
        } else {
            changed_files.iter().take(8).cloned().collect::<Vec<_>>()
        };
        let commands = self.executed_commands();
        let command_lines = if commands.is_empty() {
            vec!["команды не зафиксированы".to_string()]
        } else {
            commands.iter().take(5).cloned().collect::<Vec<_>>()
        };
        let validations = self.validation_steps();
        let validation_lines = if validations.is_empty() {
            vec!["валидация не запускалась или не распознана".to_string()]
        } else {
            validations.clone()
        };
        let risks = if self.failed {
            "есть ошибка в ходе запуска; проверьте последние шаги timeline".to_string()
        } else if validations.is_empty() {
            "нет явной проверки cargo check/test/lint/build".to_string()
        } else {
            "явных рисков не отмечено".to_string()
        };
        format!(
            "Итог запуска: {status}\nВремя: {}\n\nИзменённые файлы:\n{}\n\nКоманды:\n{}\n\nВалидация:\n{}\n\nРиски:\n- {risks}\n\nNext steps:\n- при необходимости откройте детали шагов или запустите проверку\n\nRun summary id: не сохранялся автоматически",
            self.elapsed_label(),
            bullet_list(&changed),
            bullet_list(&command_lines),
            bullet_list(&validation_lines),
        )
    }
}

fn bullet_list(items: &[String]) -> String {
    items
        .iter()
        .map(|item| format!("- {item}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn finish_step(step: &mut RunTimelineStep, status: RunTimelineStatus, message: &str) {
    if step.finished_after.is_none() {
        step.finished_after = step.started_at.map(|started| started.elapsed());
    }
    step.status = status;
    if !message.trim().is_empty() {
        append_compact(&mut step.output, message, 2_400);
    }
}

fn is_command_step(step: &RunTimelineStep) -> bool {
    let text = format!("{} {}", step.title, step.detail).to_lowercase();
    text.contains("run_shell")
        || text.contains("project_command")
        || text.contains("terminal_write")
        || text.contains("terminal_start")
}

fn is_validation_step(step: &RunTimelineStep) -> bool {
    let text = format!("{} {}", step.title, step.detail).to_lowercase();
    text.contains("cargo check")
        || text.contains("cargo test")
        || text.contains(" lint")
        || text.contains("build")
        || text.contains("test")
        || text.contains("check")
}

fn extract_act_action(summary: &str) -> Option<&str> {
    let marker = "\"action\":";
    let start = summary.find(marker)? + marker.len();
    let rest = summary[start..].trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(&rest[..end])
}

fn append_compact(target: &mut String, text: &str, max_chars: usize) {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return;
    }
    if !target.is_empty() {
        target.push('\n');
    }
    target.push_str(trimmed);
    if target.chars().count() > max_chars {
        let suffix = target
            .chars()
            .rev()
            .take(max_chars)
            .collect::<String>()
            .chars()
            .rev()
            .collect::<String>();
        *target = format!("…{suffix}");
    }
}

pub fn compact(text: &str, max_chars: usize) -> String {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= max_chars {
        normalized
    } else {
        format!(
            "{}…",
            normalized.chars().take(max_chars).collect::<String>()
        )
    }
}

pub fn format_duration(duration: Duration) -> String {
    let total_ms = duration.as_millis();
    if total_ms < 1_000 {
        format!("{total_ms} мс")
    } else if total_ms < 60_000 {
        format!("{:.1} с", total_ms as f64 / 1_000.0)
    } else {
        let minutes = total_ms / 60_000;
        let seconds = (total_ms % 60_000) / 1_000;
        format!("{minutes} мин {seconds:02} с")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_act_action_from_summary() {
        let summary = r#"act({"action":"project_command","args":{"command":"check"}})"#;
        assert_eq!(extract_act_action(summary), Some("project_command"));
    }

    #[test]
    fn report_mentions_validation_when_check_ran() {
        let mut timeline = RunTimeline::new("fix compile");
        timeline.tool_started(
            "tool-1".to_string(),
            "act".to_string(),
            r#"act({"action":"project_command","args":{"command":"cargo check"}})"#.to_string(),
        );
        timeline.tool_finished("tool-1", "exit code 0");
        timeline.finish(&["src/app.rs".to_string()]);
        let report = timeline.final_report.expect("report");
        assert!(report.contains("Валидация:"));
        assert!(report.contains("project_command"));
        assert!(report.contains("src/app.rs"));
    }
}
