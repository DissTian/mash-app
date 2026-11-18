//! Automatic attack-chain selection for takeover mode.
//!
//! This module is pure decision logic: it receives the recognized command
//! cards, the three NP slots, and the resolved strategy, and returns the
//! ordered picks the runner should tap. No device I/O happens here, which
//! keeps every policy branch unit-testable.

use super::*;

fn color_code(color: CommandCardColor) -> &'static str {
    match color {
        CommandCardColor::Buster => "b",
        CommandCardColor::Arts => "a",
        CommandCardColor::Quick => "q",
    }
}

fn np_pick(np: &NoblePhantasmMatch) -> Pick {
    Pick::Np {
        slot: np.slot,
        point: rect_center(&np.card_region),
        from_priority: "自动战斗：宝具".into(),
    }
}

/// A ready NP occupies one of the five physical card slots. Treating it as a
/// normal card would double-tap the same slot, so it never enters the card
/// candidate pool.
fn card_hits_ready_np(card: &CommandCardMatch, nps: &[NoblePhantasmMatch]) -> bool {
    nps.iter().any(|np| {
        np.ready
            && card.x >= np.card_region.x
            && card.x <= np.card_region.x + np.card_region.w
            && card.y >= np.card_region.y
            && card.y <= np.card_region.y + np.card_region.h
    })
}

fn card_pick(card: &CommandCardMatch) -> Pick {
    Pick::Card {
        slot: card.slot,
        point: Point::new(card.x, card.y),
        servant_id: card.servant_id,
        suit: card.suit.clone(),
        from_priority: Some("自动战斗".into()),
    }
}

fn eligible_cards<'a>(
    cards: &'a [&'a CommandCardMatch],
    nps: &[NoblePhantasmMatch],
    used: &HashSet<u32>,
) -> Vec<&'a CommandCardMatch> {
    let mut eligible: Vec<&CommandCardMatch> = cards
        .iter()
        .copied()
        .filter(|card| !used.contains(&card.slot) && !card_hits_ready_np(card, nps))
        .collect();
    eligible.sort_by_key(|card| card.slot);
    eligible
}

fn push_card(picks: &mut Vec<Pick>, used: &mut HashSet<u32>, card: &CommandCardMatch) {
    used.insert(card.slot);
    picks.push(card_pick(card));
}

/// Pick `need` cards following the configured color policy, then fill any
/// remainder with the leftmost actionable cards.
fn choose_cards(
    cards: &[&CommandCardMatch],
    nps: &[NoblePhantasmMatch],
    need: usize,
    strategy: &CommandCardStrategy,
    used: &mut HashSet<u32>,
) -> Vec<Pick> {
    let mut picks = Vec::with_capacity(need);
    let mut eligible = eligible_cards(cards, nps, used);

    match strategy.mode {
        CommandCardMode::Leftmost => {}
        CommandCardMode::Rainbow => {
            for color in &strategy.color_order {
                if picks.len() >= need {
                    break;
                }
                let code = color_code(*color);
                if let Some(card) = eligible
                    .iter()
                    .find(|card| card.suit.as_deref() == Some(code))
                    .copied()
                {
                    push_card(&mut picks, used, card);
                    eligible.retain(|candidate| candidate.slot != card.slot);
                }
            }
        }
        CommandCardMode::SameColor => {
            let remaining = need.saturating_sub(picks.len());
            let mut complete = false;
            if remaining >= 2 {
                for color in &strategy.color_order {
                    let code = color_code(*color);
                    let matching: Vec<&CommandCardMatch> = eligible
                        .iter()
                        .copied()
                        .filter(|card| card.suit.as_deref() == Some(code))
                        .collect();
                    if matching.len() >= remaining {
                        for card in matching.iter().take(remaining) {
                            push_card(&mut picks, used, card);
                        }
                        complete = true;
                        break;
                    }
                }
            }
            if !complete && picks.len() < need {
                // No complete monochrome chain: take every card of the most
                // plentiful color, then let the leftmost fill complete it.
                let mut best: Option<(usize, CommandCardColor)> = None;
                for color in &strategy.color_order {
                    let code = color_code(*color);
                    let count = eligible
                        .iter()
                        .filter(|card| card.suit.as_deref() == Some(code))
                        .count();
                    if count == 0 {
                        continue;
                    }
                    if best.is_none_or(|(best_count, _)| count > best_count) {
                        best = Some((count, *color));
                    }
                }
                if let Some((_, color)) = best {
                    let code = color_code(color);
                    let take = need.saturating_sub(picks.len());
                    let selected: Vec<&CommandCardMatch> = eligible
                        .iter()
                        .copied()
                        .filter(|card| card.suit.as_deref() == Some(code))
                        .take(take)
                        .collect();
                    for card in selected {
                        push_card(&mut picks, used, card);
                    }
                    eligible.retain(|candidate| !used.contains(&candidate.slot));
                }
            }
        }
    }

    for card in eligible {
        if picks.len() >= need {
            break;
        }
        if used.contains(&card.slot) {
            continue;
        }
        push_card(&mut picks, used, card);
    }
    picks
}

/// Build the ordered three-pick chain for one automatic attack turn.
pub(crate) fn auto_attack_picks(
    cards: &[CommandCardMatch],
    nps: &[NoblePhantasmMatch],
    strategy: &AutoBattleStrategy,
) -> Vec<Pick> {
    let actionable: Vec<&CommandCardMatch> = cards.iter().filter(|card| !card.is_stunned).collect();
    let ready: Vec<&NoblePhantasmMatch> = nps.iter().filter(|np| np.ready).collect();
    let max_count = usize::from(strategy.noble_phantasm.max_count.clamp(1, 3));
    let mut picks: Vec<Pick> = Vec::with_capacity(3);
    let mut used_cards: HashSet<u32> = HashSet::new();

    let wants_last_np =
        strategy.noble_phantasm.mode == NoblePhantasmUseMode::LastCard && !ready.is_empty();
    match strategy.noble_phantasm.mode {
        NoblePhantasmUseMode::Off => {}
        NoblePhantasmUseMode::AllReady => {
            for np in ready.iter().take(max_count) {
                picks.push(np_pick(np));
            }
        }
        NoblePhantasmUseMode::Single => {
            if let Some(np) = ready.first() {
                picks.push(np_pick(np));
            }
        }
        NoblePhantasmUseMode::LastCard => {}
    }

    let card_target = if wants_last_np {
        2
    } else {
        3usize.saturating_sub(picks.len())
    };
    if card_target > 0 {
        let chosen = choose_cards(
            &actionable,
            nps,
            card_target,
            &strategy.command_card,
            &mut used_cards,
        );
        picks.extend(chosen);
    }

    if wants_last_np && picks.len() < 3 {
        if let Some(np) = ready.first() {
            picks.push(np_pick(np));
        }
    }

    if picks.len() < 3 {
        let mut eligible = eligible_cards(&actionable, nps, &used_cards);
        for card in eligible.drain(..) {
            if picks.len() >= 3 {
                break;
            }
            push_card(&mut picks, &mut used_cards, card);
        }
    }

    picks.truncate(3);
    picks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(slot: u32, suit: &str) -> CommandCardMatch {
        CommandCardMatch {
            slot,
            x: 0.1 + slot as f64 * 0.1,
            y: 0.7,
            card_region: NormRect {
                x: 0.1 + slot as f64 * 0.1,
                y: 0.6,
                w: 0.05,
                h: 0.2,
            },
            face_region: NormRect {
                x: 0.1 + slot as f64 * 0.1,
                y: 0.6,
                w: 0.05,
                h: 0.1,
            },
            crit_digit_regions: None,
            crit_digit_reads: None,
            suit: Some(suit.into()),
            icon_score: None,
            icon_region: None,
            servant_id: None,
            is_support: false,
            is_stunned: false,
            ascension: None,
            face_score: None,
            crit_chance: None,
            support_icon_score: None,
            support_icon_region: None,
        }
    }

    fn np(slot: u32, ready: bool, y: f64) -> NoblePhantasmMatch {
        NoblePhantasmMatch {
            slot,
            card_region: NormRect {
                x: 0.1 + slot as f64 * 0.1,
                y,
                w: 0.05,
                h: 0.2,
            },
            ready,
            edge_frac: 0.0,
            std_bgr: 0.0,
            edge_threshold: 0.0,
            card_ready: None,
            ready_source: None,
            gauge_digit_count: None,
            gauge_hundreds_visible: None,
            gauge_digit_model_labels: None,
            turn_count_model_label: None,
            gauge_sequence_value: None,
            gauge_sequence_confidence: None,
            gauge_sequence_accepted: None,
            turn_sequence_value: None,
            turn_sequence_confidence: None,
            turn_sequence_accepted: None,
            gauge_region: None,
            np_glow_region: None,
            np_glow_score: None,
            np_glow_ready: None,
        }
    }

    fn strategy() -> AutoBattleStrategy {
        AutoBattleStrategy::default()
    }

    #[test]
    fn same_color_takes_three_of_the_first_available_color() {
        let cards = vec![
            card(0, "b"),
            card(1, "b"),
            card(2, "q"),
            card(3, "b"),
            card(4, "a"),
        ];
        let picks = auto_attack_picks(&cards, &[], &strategy());
        assert_eq!(picks.len(), 3);
        let slots: Vec<u32> = picks
            .iter()
            .map(|pick| match pick {
                Pick::Card { slot, .. } => *slot,
                Pick::Np { .. } => u32::MAX,
            })
            .collect();
        assert_eq!(slots, [0, 1, 3]);
    }

    #[test]
    fn same_color_falls_back_to_best_color_then_leftmost() {
        let cards = vec![
            card(0, "b"),
            card(1, "a"),
            card(2, "q"),
            card(3, "b"),
            card(4, "a"),
        ];
        let picks = auto_attack_picks(&cards, &[], &strategy());
        let slots: Vec<u32> = picks
            .iter()
            .map(|pick| match pick {
                Pick::Card { slot, .. } => *slot,
                Pick::Np { .. } => u32::MAX,
            })
            .collect();
        // 2 buster + leftmost remaining (slot 1 arts).
        assert_eq!(slots, [0, 3, 1]);
    }

    #[test]
    fn rainbow_takes_one_of_each_color() {
        let mut strategy = strategy();
        strategy.command_card.mode = CommandCardMode::Rainbow;
        let cards = vec![
            card(0, "b"),
            card(1, "b"),
            card(2, "q"),
            card(3, "b"),
            card(4, "a"),
        ];
        let picks = auto_attack_picks(&cards, &[], &strategy);
        let slots: Vec<u32> = picks
            .iter()
            .map(|pick| match pick {
                Pick::Card { slot, .. } => *slot,
                Pick::Np { .. } => u32::MAX,
            })
            .collect();
        assert_eq!(slots, [0, 4, 2]);
    }

    #[test]
    fn ready_noble_phantasms_lead_the_chain_and_are_not_reused_as_cards() {
        let cards = vec![
            card(0, "b"),
            card(1, "a"),
            card(2, "q"),
            card(3, "b"),
            card(4, "a"),
        ];
        // NP slot 0 covers card slot 0.
        let nps = vec![np(0, true, 0.6), np(1, false, 0.6), np(2, false, 0.6)];
        let picks = auto_attack_picks(&cards, &nps, &strategy());
        assert!(matches!(&picks[0], Pick::Np { slot: 0, .. }));
        let card_slots: Vec<u32> = picks[1..]
            .iter()
            .map(|pick| match pick {
                Pick::Card { slot, .. } => *slot,
                Pick::Np { .. } => u32::MAX,
            })
            .collect();
        assert_eq!(card_slots.len(), 2);
        assert!(!card_slots.contains(&0));
    }

    #[test]
    fn np_off_ignores_ready_noble_phantasms() {
        let mut strategy = strategy();
        strategy.noble_phantasm.mode = NoblePhantasmUseMode::Off;
        let cards = vec![
            card(0, "b"),
            card(1, "a"),
            card(2, "q"),
            card(3, "b"),
            card(4, "a"),
        ];
        let nps = vec![np(0, true, 0.6), np(1, true, 0.6), np(2, true, 0.6)];
        let picks = auto_attack_picks(&cards, &nps, &strategy);
        assert!(picks.iter().all(|pick| matches!(pick, Pick::Card { .. })));
    }

    #[test]
    fn np_last_card_places_the_noble_phantasm_third() {
        let mut strategy = strategy();
        strategy.noble_phantasm.mode = NoblePhantasmUseMode::LastCard;
        let cards = vec![
            card(0, "b"),
            card(1, "a"),
            card(2, "q"),
            card(3, "b"),
            card(4, "a"),
        ];
        let nps = vec![np(2, true, 0.6), np(0, false, 0.6), np(1, false, 0.6)];
        let picks = auto_attack_picks(&cards, &nps, &strategy);
        assert!(matches!(&picks[2], Pick::Np { slot: 2, .. }));
    }

    #[test]
    fn stunned_cards_and_unready_slots_are_skipped() {
        let mut cards = vec![
            card(0, "b"),
            card(1, "b"),
            card(2, "b"),
            card(3, "a"),
            card(4, "q"),
        ];
        cards[1].is_stunned = true;
        let picks = auto_attack_picks(&cards, &[], &strategy());
        let slots: Vec<u32> = picks
            .iter()
            .map(|pick| match pick {
                Pick::Card { slot, .. } => *slot,
                Pick::Np { .. } => u32::MAX,
            })
            .collect();
        assert_eq!(slots, [0, 2, 3]);
    }
}
