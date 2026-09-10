//! Per-topic lessons: the concrete 预习 material behind a roadmap topic.
//!
//! A roadmap lists topic *names*; a [`Lesson`] is the course *content* for one of
//! them — a short overview plus a handful of practice problems (题目), each with a
//! worked solution (题解). Lessons live beside the course as hand-editable
//! Markdown (`<dir>/.tutor/lessons/<id>.md`), drafted on demand by the brain and
//! cached, so they are portable and git-friendly like `roadmap.md`.
//!
//! Parsing is deliberately lenient and *word-agnostic*: `## ` opens a problem and
//! the first `### ` inside it splits prompt from solution, regardless of the
//! heading text — so a Chinese lesson (`## 题目 1` / `### 题解`) and an English one
//! (`## Problem 1` / `### Solution`) parse identically.

/// One practice problem and its worked solution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    pub title: String,
    pub prompt: String,
    pub solution: String,
}

/// A topic's lesson: an overview and its ordered problems.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lesson {
    pub topic: String,
    pub overview: String,
    pub problems: Vec<Problem>,
}

impl Lesson {
    /// Parse a lesson Markdown document. The first `# ` heading is the topic; the
    /// text before the first `## ` is the overview; each `## ` opens a problem
    /// whose first `### ` divides the prompt from the solution.
    pub fn parse(md: &str, topic_fallback: &str) -> Lesson {
        let mut topic = topic_fallback.trim().to_string();
        let mut seen_title = false;
        let mut overview = String::new();
        let mut problems: Vec<Problem> = Vec::new();
        let mut cur: Option<Problem> = None;
        let mut in_solution = false;

        for raw in md.lines() {
            let line = raw.trim_end();
            let trimmed = line.trim_start();

            if let Some(rest) = trimmed.strip_prefix("## ") {
                if let Some(p) = cur.take() {
                    problems.push(finish(p));
                }
                cur = Some(Problem {
                    title: rest.trim().to_string(),
                    prompt: String::new(),
                    solution: String::new(),
                });
                in_solution = false;
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("# ") {
                if !seen_title {
                    let t = rest.trim();
                    if !t.is_empty() {
                        topic = t.to_string();
                    }
                    seen_title = true;
                    continue;
                }
                // A stray top-level heading after the title: treat as content.
            }
            // The first `### ` inside a problem divides prompt from solution.
            if trimmed.starts_with("### ") && cur.is_some() && !in_solution {
                in_solution = true;
                continue;
            }

            match cur.as_mut() {
                Some(p) => {
                    let buf = if in_solution {
                        &mut p.solution
                    } else {
                        &mut p.prompt
                    };
                    buf.push_str(line);
                    buf.push('\n');
                }
                None => {
                    // Preamble → overview, dropping a leading blockquote note.
                    if !(overview.is_empty() && trimmed.starts_with('>')) {
                        overview.push_str(line);
                        overview.push('\n');
                    }
                }
            }
        }
        if let Some(p) = cur.take() {
            problems.push(finish(p));
        }
        Lesson {
            topic,
            overview: overview.trim().to_string(),
            problems,
        }
    }

    /// True when the lesson carries no usable content (e.g. a blank file).
    pub fn is_empty(&self) -> bool {
        self.overview.is_empty() && self.problems.is_empty()
    }

    /// Render back to the canonical Markdown template (round-trips [`parse`]).
    pub fn to_markdown(&self) -> String {
        let mut s = format!("# {}\n\n", self.topic);
        if !self.overview.is_empty() {
            s.push_str(&self.overview);
            s.push_str("\n\n");
        }
        for (i, p) in self.problems.iter().enumerate() {
            let title = if p.title.is_empty() {
                format!("题目 {}", i + 1)
            } else {
                p.title.clone()
            };
            s.push_str(&format!("## {title}\n\n"));
            if !p.prompt.is_empty() {
                s.push_str(&p.prompt);
                s.push_str("\n\n");
            }
            s.push_str("### 题解\n\n");
            if !p.solution.is_empty() {
                s.push_str(&p.solution);
                s.push_str("\n\n");
            }
        }
        s.trim_end().to_string()
    }
}

fn finish(mut p: Problem) -> Problem {
    p.prompt = p.prompt.trim().to_string();
    p.solution = p.solution.trim().to_string();
    p
}

/// The prompt handed to `omp -p` to draft a lesson for one topic. `lang` is a
/// natural-language instruction (supplied by the presentation layer) telling the
/// model which language to write in. `code_lang`, when set, pins the programming
/// language used in every code example; when `None` the model chooses.
pub fn lesson_prompt(subject: &str, topic: &str, lang: &str, code_lang: Option<&str>) -> String {
    let code = match code_lang {
        Some(c) => {
            format!("Write EVERY code example in {c}. Use idiomatic {c} in a fenced code block.")
        }
        None => "Use whichever popular programming language fits best.".to_string(),
    };
    format!(
        "Create a focused practice lesson for the topic \"{topic}\" within the \
         subject \"{subject}\". {lang} {code}\n\n\
         Output ONLY GitHub-Flavored Markdown, nothing else, in exactly this shape:\n\
         # {topic}\n\n\
         <one short paragraph: what this topic is and why it matters>\n\n\
         ## 题目 1\n\
         <a concrete, self-contained practice problem statement>\n\n\
         ### 题解\n\
         <a clear worked solution: the key idea, the steps, and the answer>\n\n\
         ## 题目 2\n\
         <...>\n\n\
         Provide 3 to 5 problems of increasing difficulty. Every problem MUST have \
         a full worked solution under its own `### ` heading. Keep it concrete and \
         practical (include short code or worked steps where useful). Do not add \
         any commentary outside this structure."
    )
}

/// A minimal offline starter lesson, used when the brain is unavailable. The user
/// fills in the problem and solution by hand, then `tutor course lesson` shows it.
pub fn starter_lesson(topic: &str) -> String {
    format!(
        "# {topic}\n\n\
         > Starter lesson (offline). Write the overview, then add 题目/题解 below.\n\n\
         ## 题目 1\n\
         Describe a concrete problem for \"{topic}\".\n\n\
         ### 题解\n\
         Write the worked solution here.\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
# 二分查找

在有序数组中以 O(log n) 定位目标。

## 题目 1
在升序数组中查找 target，返回下标或 -1。

### 题解
维护 lo/hi 两个指针，每轮取中点比较。
关键：循环不变式 lo<=hi。

## 题目 2
查找第一个 >= target 的位置。

### 题解
右边界收敛到左端。
";

    #[test]
    fn parses_topic_overview_and_problems() {
        let l = Lesson::parse(SAMPLE, "fallback");
        assert_eq!(l.topic, "二分查找");
        assert_eq!(l.overview, "在有序数组中以 O(log n) 定位目标。");
        assert_eq!(l.problems.len(), 2);
        assert_eq!(l.problems[0].title, "题目 1");
        assert!(l.problems[0].prompt.contains("返回下标"));
        assert!(l.problems[0].solution.contains("循环不变式"));
        assert!(l.problems[1].solution.contains("右边界"));
    }

    #[test]
    fn topic_falls_back_when_no_heading() {
        let l = Lesson::parse("## 题目 1\nx\n### 题解\ny\n", "回退主题");
        assert_eq!(l.topic, "回退主题");
        assert_eq!(l.problems.len(), 1);
    }

    #[test]
    fn word_agnostic_english_headings_parse() {
        let md = "# Binary Search\n\nover.\n\n## Problem 1\nfind x\n### Solution\nmid\n";
        let l = Lesson::parse(md, "");
        assert_eq!(l.topic, "Binary Search");
        assert_eq!(l.problems.len(), 1);
        assert_eq!(l.problems[0].prompt, "find x");
        assert_eq!(l.problems[0].solution, "mid");
    }

    #[test]
    fn round_trips_through_markdown() {
        let l = Lesson::parse(SAMPLE, "fallback");
        let l2 = Lesson::parse(&l.to_markdown(), "fallback");
        assert_eq!(l, l2);
    }

    #[test]
    fn starter_is_parseable() {
        let l = Lesson::parse(&starter_lesson("图的遍历"), "");
        assert_eq!(l.topic, "图的遍历");
        assert_eq!(l.problems.len(), 1);
    }

    #[test]
    fn lesson_prompt_pins_code_language() {
        let pinned = lesson_prompt("algorithms", "二分查找", "中文", Some("rust"));
        assert!(pinned.contains("in rust"));
        let free = lesson_prompt("algorithms", "二分查找", "中文", None);
        assert!(!free.contains("EVERY code example"));
    }
}
