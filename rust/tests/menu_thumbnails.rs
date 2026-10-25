use clover_portable::frontend::menu_scene::{Rect, ThumbnailPlacement, thumbnails};

#[test]
fn one_and_three_game_strips_keep_exact_layout_selection_and_hit_boundaries() {
    assert_eq!(
        thumbnails(1, 0),
        vec![ThumbnailPlacement {
            index: 0,
            cover: Rect {
                x: 624.0,
                y: 535.0,
                w: 32.0,
                h: 32.0
            },
            alpha: 255,
            marker: Some(Rect {
                x: 640.0,
                y: 522.0,
                w: 24.0,
                h: 16.0
            }),
            lock: Rect {
                x: 649.0,
                y: 542.0,
                w: 14.0,
                h: 18.0
            },
            hit: Rect {
                x: 619.0,
                y: 526.0,
                w: 42.0,
                h: 55.0
            },
        }]
    );

    let plan = thumbnails(3, 1);
    let centers = [598.0, 640.0, 682.0];
    assert_eq!(plan.len(), 3);
    for (index, (placement, x)) in plan.iter().zip(centers).enumerate() {
        assert_eq!(placement.index, index);
        assert_eq!(
            placement.cover,
            Rect {
                x: x - 16.0,
                y: 535.0,
                w: 32.0,
                h: 32.0
            }
        );
        assert_eq!(placement.alpha, if index == 1 { 255 } else { 190 });
        assert_eq!(
            placement.marker,
            (index == 1).then_some(Rect {
                x,
                y: 522.0,
                w: 24.0,
                h: 16.0
            })
        );
        assert_eq!(
            placement.lock,
            Rect {
                x: x + 9.0,
                y: 542.0,
                w: 14.0,
                h: 18.0
            }
        );
        assert_eq!(
            placement.hit,
            Rect {
                x: x - 21.0,
                y: 526.0,
                w: 42.0,
                h: 55.0
            }
        );
    }
    assert_eq!(plan[0].hit.x + plan[0].hit.w, plan[1].hit.x);
    assert_eq!(plan[1].hit.x + plan[1].hit.w, plan[2].hit.x);
}

#[test]
fn dense_twenty_seven_game_strip_uses_forty_pixel_step_and_out_of_range_selection() {
    let plan = thumbnails(27, 27);
    assert_eq!(plan.len(), 27);
    for (index, placement) in plan.iter().enumerate() {
        let x = 120.0 + index as f32 * 40.0;
        assert_eq!(placement.index, index);
        assert_eq!(
            placement.cover,
            Rect {
                x: x - 16.0,
                y: 535.0,
                w: 32.0,
                h: 32.0
            }
        );
        assert_eq!(placement.alpha, 190);
        assert_eq!(placement.marker, None);
        assert_eq!(
            placement.lock,
            Rect {
                x: x + 9.0,
                y: 542.0,
                w: 14.0,
                h: 18.0
            }
        );
        assert_eq!(
            placement.hit,
            Rect {
                x: x - 20.0,
                y: 526.0,
                w: 40.0,
                h: 55.0
            }
        );
    }
    assert_eq!(plan[0].hit.x, 100.0);
    assert_eq!(plan[26].hit.x + plan[26].hit.w, 1180.0);
}

#[test]
fn empty_game_list_produces_no_thumbnail_placements() {
    assert!(thumbnails(0, 0).is_empty());
}
