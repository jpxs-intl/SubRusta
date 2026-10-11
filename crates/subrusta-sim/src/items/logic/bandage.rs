use crate::{items::state::ItemState, sim::Sim};

const BANDAGE_RANGE: f32 = 2.0;
const BANDAGE_TICKS: i32 = 255;

impl Sim {
    pub(crate) fn bandage_logic(&mut self, id: usize) {
        let item = self.items.get_mut(id).unwrap();

        let (item_pos, holder) = (self.bodies.get(item.body).map_or(item.pos2, |b| b.pos), item.parent_human);

        let mut nearest = (BANDAGE_RANGE, None);

        for (k, human) in self.humans.iter() {
            if !(human.health.vitality > 0 && (human.bleeding || human.health.vitality <= 9)) {
                continue;
            }

            let dist = human.pos.distance(item_pos);

            if nearest.0 > dist {
                nearest = (dist, Some(k));
            }
        }

        let Some(target) = nearest.1 else { return };

        let ItemState::Bandage { usage_left: left, progress } = &mut item.state else { return };
        *progress += 1;

        let p = *progress;
        if holder != -1
            && let Some(h) = self.humans.get_mut(holder as usize)
        {
            h.progress_bar = p;
        }

        if p <= BANDAGE_TICKS {
            return;
        }

        *progress = 0;
        *left -= 1;

        if *left <= 0 {
            self.mark_item_for_deletion(id);
        }

        if let Some(t) = self.humans.get_mut(target) {
            t.bleeding = false;

            if t.health.vitality <= 9 {
                t.health.vitality = 10;
            }
        }
    }
}
