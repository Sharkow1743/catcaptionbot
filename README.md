# catsbot ("коты с подписями")
This is game in telegram bot. Its main mechanic is getting random image of a cat with random, but fully correct captions

---

## 1. Design Pillars

These are the constraints every mechanic below should be checked against:

1. **No dead ends.** No player action should be able to put the player in a state they can't get out of
2. **RNG-forward, with a legible skill edge.** Battles should feel like a genuinely random draw, not a skill contest — that's what makes the outcome trustworthy for settling something real. On top of the base randomness, *adrenaline pacing* is the one deliberate skill lever, giving the more experienced player a small, learnable edge. Same philosophy should apply to the main mechanics too
3. **No free lunch.** Any action that lets a player skip a grind (e.g., releasing a bad cat for a fresh good one) must be neutral-or-worse compared to just doing the grind. Never strictly better.
4. **Punish churn, not participation.** Negative reinforcement should target repeated exploit-y behavior, not the first, normal instance of an action (releasing one unwanted cat should never feel bad).

---

## 2. Core Loop

### 2.1 Roll
Produces a cat: random image (edited per rarity), random rarity, and generated flavor text.
- **Text generation:** fully random word selection, biased by rarity (e.g., rarity sets tone/adjective pool), then grammar-corrected as a post-process pass.
- **Image generation:** edited based on rarity and current modifier state (see Adrenaline below — corruption is visual, not just numerical).

### 2.2 Catch
After a roll, the cat can be **caught** or can **escape** before catching resolves.

Escape chance scales with:
  - Rarity (higher rarity = higher base escape chance)
  - Active modifiers (adrenaline increases escape chance; see below)

### 2.3 Modifiers
All modifiers are **additive**, applied to a shared pool of relevant stats (catch chance, escape chance, rarity odds, image/text corruption).

#### Adrenaline
- Increases with every roll.
- Effects: higher chance of rare cats **and** higher chance of trash/junk cats (raises variance both directions); image/text rendering becomes visibly more corrupted as adrenaline rises.
- **Reset rule:** a successful **catch** partially resets adrenaline (reward for banking a result). An **escape** does **not** reset adrenaline — the player paid the corruption/variance cost and got nothing for it. This makes escape meaningfully worse than "just a wasted roll".

#### Loyalty (may be splitted into dedicated system)
- Only exists once the player has an **active/equipped cat**. Before any catch, rolls are neutral (no bonus, no penalty).
- Loyalty **decreases** as you roll (using your active cat to keep rolling wears on it).
- Max loyalty and regen rate scale with the active cat's rarity (rarer cats have a higher ceiling and, presumably, different regen pacing — see open question in §6).
- **Effects:**
  - Low loyalty → lowers max rollable rarity, lowers catch chance.
  - High loyalty → grants bonuses (higher rarity ceiling, better catch chance).
- **Regen condition (recommended default):** loyalty regenerates primarily while a cat is **benched** (unequipped), establishing the roster/slot system as a real recovery queue, not just storage. (Confirm vs. a slower always-on regen — see §6.)

---

## 3. Roster & Slot System

### 3.1 Slots
- Players have a **limited number of cat slots**. This is the core resource-management constraint of the meta-loop.

### 3.2 Loyalty-Gated Actions — asymmetric by design

This is the load-bearing decision of the whole roster system. **Do not gate every roster action against loyalty in the same direction.** A cat's loyalty is naturally lowest exactly when a player most wants to remove or bench it — if every action requires *high* loyalty to succeed, the worst cat becomes the hardest one to get rid of, which is a softlock inside a limited-slot system.

| Action | Gate direction | Rationale |
|---|---|---|
| **Release** (permanent) | **Inverted** — low loyalty = easy, high loyalty = hard | Thematically clean ("hard to let go of a cat that loves you"). Mechanically ensures the cat you most want gone is always the easy case. Prevents worst-case slot lock. |
| **Unequip** (bench, non-permanent) | **Near-unconditional** — token/cosmetic check at most, no real failure chance | Must always be reachable, or the loyalty-regen loop (which needs bench time) becomes unreachable for exactly the cats that need it most. |
| **Equip** (bring into active slot) | **Normal** — low loyalty = hard, high loyalty = easy | Safe to gate hard here because failure has no trap state — worst case, your current active cat stays active one more roll. This is where the intended strategic tension lives: a cat you've ground down becomes reluctant to come back out, incentivizing resting it *before* it's fully depleted. |

### 3.3 Full-slot catch handling
When a player catches a new cat with no free slot, use a **temporary holding pen**:
- The new catch waits outside the slot system for a defined window.
- If the player frees a slot within that window, the cat can be placed.
- If not, the cat leaves.

This preserves catch stakes (a successful, RNG-beaten catch is never instantly voided) without requiring a forced same-instant release decision, which risks compounding two RNG layers (catch RNG + a release-check that could itself fail) into one bad moment.

*(Rejected alternatives: forced immediate release-or-lose choice — reopens softlock risk if the release check fails at the worst time; hard block on catching while full — too punishing, discourages engaging with rare-hunting when roster happens to be full.)*

### 3.4 Timeout mechanics (on any failed gated action)
Needs three parameters defined and tuned:
1. **Cooldown scaling:** start flat (simplest); consider scaling with margin-of-failure or consecutive fails later for a natural pity curve.
2. **Failure side-effect:** a failed attempt should cost *something* beyond the wait, so spam-retrying at cooldown-zero isn't free. Suggested: failed release nudges that cat's loyalty up slightly; failed equip costs a sliver of adrenaline.
3. **Pity counter:** repeated failures should eventually guarantee success (soft pity), especially for **release**, since so much of the anti-softlock design depends on release being reliably reachable.

---

## 4. Anti-Exploit: Loyalty Laundering

**The exploit:** release a low-loyalty cat (which only gives debuffs) and catch a new one, which — if new cats start at max loyalty — immediately grants bonuses. This makes "release and recatch" strictly dominant over "rest and recover," undermining the entire loyalty system.

**Fix: new catches start at neutral loyalty**, not max — the same neutral state a player is in before their first-ever catch. No bonus, no penalty on arrival.

Why this works:
- Release-and-recatch no longer skips the grind — it resets the player to the *same* starting line as resting out their current cat's regen. The choice becomes a genuine trade-off (catch-RNG risk + time vs. rest time), not a shortcut.
- As a side benefit, it makes **rarity matter independently of loyalty from turn one**: a rare cat's higher loyalty ceiling is now a real asset immediately, not something that only matters after the player has already ground the cat down once.

**Optional reinforcement layers:**
- **Small debuff on catch**, instead of pure neutral. Makes churning strictly *worse* than patiently resting (currently a wash). See constraints below.
- **Slot cooldown on release** — the *slot itself* locks briefly after a release (flat timer, not loyalty-gated). Adds a real time cost to churn independent of loyalty math.
- **Soft attachment decay** — releasing multiple cats in a short window temporarily raises the next catch's escape chance, decaying back to normal over time. Only bites repeat-churners; a single normal release is unaffected.

**Debuff-on-catch constraints, if implemented:**
- Must be **flat and small**, and must **not scale with rarity**. A rare catch already beat worse odds to be caught at all — if it also starts more debuffed (or debuffed at all differently) than a common, the "I got the rare one!" moment gets undercut.
- Should be shallow enough to only matter in edge-case math (marginal odds), not something a player feels turn-to-turn — new catches should still feel good to equip and use immediately after catching, not artificially discouraged from being shown off.
- **Recommended sequencing:** ship neutral-start alone first. It's the change that removes the exploit's underlying incentive rather than taxing it after the fact — a tax can still be worth paying if the underlying exploit is lucrative enough; removing the free bonus means there's nothing left to pay for. Add debuff-on-catch and/or churn taxes only if churning persists.
- **Do not stack all three reinforcement layers at once** even if adding them — three negative layers on the same action risks reading as the system being punitive rather than balanced, which is a particular risk given battle mode leans on this economy for real dispute resolution (see §5).

---

## 5. Battles

Strictly 1v1. Modes: vs. bot, vs. friend (link invite), or random/matchmaking. Primary use case: resolving IRL disputes fairly — but "fairly" here means *symmetric randomness*, not *low variance*. Battles should read as RNG-driven, with adrenaline pacing layered on top as a modest, learnable edge rather than the sole determinant.

### 5.1 Objective
Players race to **find** a target rarity (or that rarity-and-above) before their opponent, within a time window. There also should be different game modes.

### 5.2 Random battle factors
Beyond adrenaline, battles should layer in additional random elements so the outcome doesn't read as purely skill-determined — this is intentional, not a fairness gap, given the dispute-resolution use case wants a *trustworthy coinflip* more than a *fair contest*. Candidates:
- **Random events mid-battle** (e.g., a temporary rarity-odds swing, a one-off free reroll, a brief adrenaline surge/drop) triggered on a timer or roll-count basis.
- **Random per-match modifiers**, rolled once at battle start and applied identically to both players (e.g., "rarity odds boosted this match," "adrenaline builds faster this match") — adds match-to-match variety without breaking symmetry.

**Hard constraint: every random factor must apply identically to both players in the same match.** Randomness *between* matches is desirable (keeps things fresh); randomness that's asymmetric *within* a match (favors one player over the other by chance) reintroduces exactly the kind of unfairness §5.5 and §5.6 are designed to prevent. Treat this as the dividing line between "more RNG" (good, wanted) and "less fair" (not wanted) — they are not the same thing.

### 5.4 Timeout / no-result resolution
No-winner draws don't work for dispute resolution. Use a **hierarchy**, not a coin flip, as the primary resolution:

1. **Highest rarity found during the match wins.**
2. **Tie on rarity → earlier find wins** (rewards being ahead, not just eventually catching up).

### 5.5 Clean-state requirement
Battles **must reset all modifiers to a clean baseline at battle start**, regardless of either player's solo-progression state:
- Both players start at **adrenaline 0**.
- **No loyalty carry-in**, regardless of active cat or roster state.
- A veteran player's meta-progression must not give them an edge over a newer player in a battle meant to fairly settle something real — this would undermine the entire premise of the feature.

### 5.6 Roster lock during battle
**Roster/equip actions should be locked out entirely once a battle is queued or started.** Otherwise the equip-gate (loyalty-based, can fail) becomes an *asymmetric* RNG layer stacked onto battle fairness — violating the symmetric-randomness constraint in §5.3.1. Whatever cat is active when the battle starts is locked for its duration.

---

# Edits
 - Maybe let loyality modifier in the battles. Because that will make players want to progress — to win in battles more often

 # TODO
