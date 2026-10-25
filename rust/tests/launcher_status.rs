use clover_portable::frontend::launcher_scene::{DrawCommand, FontRole, Rgba, TextCommand};
use clover_portable::frontend::launcher_status::StatusText;

const MUTED: Rgba = Rgba {
    r: 156,
    g: 171,
    b: 186,
    a: 255,
};
const ERROR: Rgba = Rgba {
    r: 255,
    g: 163,
    b: 156,
    a: 255,
};

fn command(value: &str, y: f32, error: bool) -> DrawCommand {
    DrawCommand::Text(TextCommand {
        value: value.to_owned(),
        x: 272.0,
        y,
        size: 15,
        font: FontRole::Regular,
        color: if error { ERROR } else { MUTED },
        max_width: 550.0,
    })
}

#[test]
fn empty_and_whitespace_statuses_keep_the_terminal_empty_request() {
    for status in ["", " \t\n\u{2003}\r\u{a0}"] {
        let policy = StatusText::new(status, false);
        assert_eq!(policy.candidate(), None);
        assert_eq!(policy.finish(), command("", 657.0, false));
    }
    let policy = StatusText::new("", true);
    assert_eq!(policy.finish(), command("", 657.0, true));
}

#[test]
fn candidates_preserve_leading_space_and_unicode_split_whitespace() {
    let mut policy = StatusText::new("\tα\u{2003}β  γ", false);
    assert_eq!(policy.candidate(), Some(" α"));
    assert_eq!(policy.advance(0.0), None);
    assert_eq!(policy.candidate(), Some("α β"));
    assert_eq!(policy.advance(0.0), None);
    assert_eq!(policy.candidate(), Some("α β γ"));
    assert_eq!(policy.advance(0.0), None);
    assert_eq!(policy.candidate(), None);
    assert_eq!(policy.finish(), command("α β γ", 657.0, false));
}

#[test]
fn strict_width_boundary_and_overlong_first_word_match_legacy_behavior() {
    let mut fits = StatusText::new("first second", false);
    assert_eq!(fits.advance(0.0), None);
    assert_eq!(fits.advance(550.0), None);
    assert_eq!(fits.finish(), command("first second", 657.0, false));

    let mut exact = StatusText::new("first second", true);
    assert_eq!(exact.candidate(), Some(" first"));
    assert_eq!(exact.advance(550.0), None);
    assert_eq!(exact.candidate(), Some("first second"));
    assert_eq!(exact.advance(551.0), Some(command("first", 657.0, true)));
    assert_eq!(exact.finish(), command("second", 678.0, true));

    let long = "x".repeat(600);
    let status = format!("{long} tail");
    let mut overlong = StatusText::new(&status, false);
    assert_eq!(overlong.candidate(), Some(format!(" {long}").as_str()));
    assert_eq!(
        overlong.advance(551.0),
        None,
        "first word never wraps by itself"
    );
    assert_eq!(overlong.candidate(), Some(format!("{long} tail").as_str()));
    assert_eq!(overlong.advance(551.0), Some(command(&long, 657.0, false)));
    assert_eq!(overlong.finish(), command("tail", 678.0, false));
}

#[test]
fn cutoff_flushes_three_lines_then_keeps_empty_final_line_without_reading_more() {
    let long = "w".repeat(600);
    let status = format!("{long} {long} {long} {long} unseen");
    let mut policy = StatusText::new(&status, true);
    let mut measured = Vec::new();
    let mut drawn = Vec::new();
    while let Some(candidate) = policy.candidate() {
        measured.push(candidate.to_owned());
        if let Some(line) = policy.advance(551.0) {
            drawn.push(line);
        }
    }
    assert_eq!(measured.len(), 4);
    assert_eq!(measured[0], format!(" {long}"));
    assert_eq!(measured[1], format!("{long} {long}"));
    assert_eq!(measured[2], format!("{long} {long}"));
    assert_eq!(measured[3], format!("{long} {long}"));
    assert_eq!(
        drawn,
        vec![
            command(&long, 657.0, true),
            command(&long, 678.0, true),
            command(&long, 699.0, true),
        ]
    );
    assert_eq!(policy.candidate(), None);
    assert_eq!(policy.finish(), command("", 720.0, true));
}

#[test]
fn nan_infinity_negative_width_and_exhausted_advance_keep_scalar_comparison_semantics() {
    let mut nan = StatusText::new("a b", false);
    assert_eq!(nan.advance(f32::NAN), None);
    assert_eq!(nan.advance(-1.0), None);
    assert_eq!(nan.finish(), command("a b", 657.0, false));

    let mut infinity = StatusText::new("a b", false);
    assert_eq!(infinity.advance(f32::INFINITY), None);
    assert_eq!(
        infinity.advance(f32::INFINITY),
        Some(command("a", 657.0, false))
    );
    assert_eq!(infinity.finish(), command("b", 678.0, false));

    let mut exhausted = StatusText::new("only", false);
    assert_eq!(exhausted.advance(0.0), None);
    assert_eq!(exhausted.candidate(), None);
    assert_eq!(exhausted.advance(f32::INFINITY), None);
    assert_eq!(exhausted.finish(), command("only", 657.0, false));
}

#[test]
fn emitted_and_final_requests_own_text_after_input_is_dropped() {
    let (emitted, final_line) = {
        let status = format!("{} next", "z".repeat(600));
        let mut policy = StatusText::new(&status, false);
        assert_eq!(policy.advance(0.0), None);
        let emitted = policy.advance(551.0).unwrap();
        let final_line = policy.finish();
        (emitted, final_line)
    };
    assert_eq!(emitted, command(&"z".repeat(600), 657.0, false));
    assert_eq!(final_line, command("next", 678.0, false));
}

fn synthetic_width(candidate: &str) -> f32 {
    candidate
        .chars()
        .map(|character| match character {
            '界' => 47.0,
            ' ' => 3.0,
            _ => 11.0,
        })
        .sum()
}

/// Literal characterization of the original native wrapping loop, including
/// the order of measurements and line draws.
fn legacy_trace(status: &str, error: bool) -> (Vec<String>, Vec<DrawCommand>) {
    let mut measured = Vec::new();
    let mut drawn = Vec::new();
    let mut line = String::new();
    let mut y = 657.0;
    for word in status.split_whitespace() {
        let candidate = format!("{line} {word}");
        measured.push(candidate.clone());
        if synthetic_width(&candidate) > 550.0 && !line.is_empty() {
            drawn.push(command(&line, y, error));
            y += 21.0;
            line.clear();
            if y > 700.0 {
                break;
            }
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    drawn.push(command(&line, y, error));
    (measured, drawn)
}

#[test]
fn deterministic_handshake_trace_matches_literal_original_loop() {
    let repeated = "a".repeat(80);
    let inputs = [
        "",
        "  one\ttwo\u{2003}three  ",
        "界界界界界界界界界界 alpha beta gamma delta epsilon zeta eta theta",
        "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu",
        repeated.as_str(),
    ];
    for (index, status) in inputs.iter().enumerate() {
        let error = index % 2 == 1;
        let (expected_measured, expected_drawn) = legacy_trace(status, error);
        let mut policy = StatusText::new(status, error);
        let mut measured = Vec::new();
        let mut drawn = Vec::new();
        while let Some(candidate) = policy.candidate() {
            measured.push(candidate.to_owned());
            if let Some(line) = policy.advance(synthetic_width(candidate)) {
                drawn.push(line);
            }
        }
        drawn.push(policy.finish());
        assert_eq!(measured, expected_measured, "status={status:?}");
        assert_eq!(drawn, expected_drawn, "status={status:?}");
    }
}

#[test]
fn draw_failure_stops_host_measurements_before_another_candidate() {
    let status = format!("{} next third", "q".repeat(600));
    let mut policy = StatusText::new(&status, false);
    let mut measurements = Vec::new();
    let mut failed = false;
    while let Some(candidate) = policy.candidate() {
        measurements.push(candidate.to_owned());
        let emitted = policy.advance(if measurements.len() == 1 { 0.0 } else { 551.0 });
        if emitted.is_some() {
            failed = true; // Simulated host draw error; propagate by stopping.
            break;
        }
    }
    assert!(failed);
    assert_eq!(measurements.len(), 2);
    assert_eq!(measurements[1], format!("{} next", "q".repeat(600)));
}
