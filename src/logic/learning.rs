use crate::bots::flaud::DECAY;

const REWARD_WEIGHT: f32 = 0.05;
const W_MAX: f32 = 0.1;

struct Synapse { 
    pre: usize, 
    post: usize, 
    w: f32, 
    e:f32 
}

fn step(syn: &mut [Synapse], pre_tr: &[f32], post_tr: &[f32], pre_spk: &[bool], post_spk: &[bool]) {
    for s in syn {
        s.e *= DECAY;
        if post_spk[s.post] {
            s.e += pre_tr[s.pre]
        }
        if pre_spk[s.pre] {
            s.e -= post_tr[s.post]
        }
    }
}

fn apply_reward(syn: &mut [Synapse], r: f32, r_bar: &mut f32, eta: f32) {
    let d = r - *r_bar;
    *r_bar += REWARD_WEIGHT * (r - *r_bar);
    _ = syn.iter_mut().map(|syn| syn.w = (syn.w + eta * d * syn.e).clamp(0.0, W_MAX));
}