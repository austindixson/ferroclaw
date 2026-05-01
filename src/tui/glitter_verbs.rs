//! Human-readable status labels for orchestrator phases.
//! Uses user-provided playful verbs instead of default Hermes wording.

use std::time::Instant;

/// Glitter verb for the prepare phase.
pub fn glitter_verb_for_prepare() -> &'static str {
    "Preparing session…"
}

const SHIMMER_GLYPHS: &[&str] = &["✦", "✧", "⋆", "✶", "✷", "✸", "✺", "✹"];

#[allow(dead_code)]
const KAOMOJI: &[&str] = &[
    "( ͡° ͜ʖ ͡°)",
    "(¬‿¬)",
    "(•̀ᴗ•́)و",
    "(ง'̀-'́)ง",
    "(づ｡◕‿‿◕｡)づ",
    "(☞ﾟヮﾟ)☞",
    "(ﾉ◕ヮ◕)ﾉ*:･ﾟ✧",
    "(｡•̀ᴗ-)✧",
    "(⌐■_■)",
    "(っ◔◡◔)っ",
    "¯\\_(ツ)_/¯",
    "(ᵔᴥᵔ)",
    "(☞ﾟ∀ﾟ)☞",
    "(✿◠‿◠)",
    "(ง •̀_•́)ง",
];

/// LLM pending verbs - derived from your custom action list (no default Hermes set).
#[allow(dead_code)]
const LLM_PENDING_VERBS: &[&str] = &[
    "Actioning",
    "Architecting",
    "Boondoggling",
    "Brewing",
    "Cascading",
    "Cerebrating",
    "Concocting",
    "Crystallizing",
    "Deciphering",
    "Elucidating",
    "Forging",
    "Grokking",
    "Harmonizing",
    "Inferring",
    "Manifesting",
    "Musing",
    "Orchestrating",
    "Percolating",
    "Pondering",
    "Processing",
    "Ruminating",
    "Synthesizing",
    "Tinkering",
    "Transmuting",
    "Warping",
    "Wrangling",
];
/// User-requested tool/action verbs for dynamic shuffle on tool calls.
const TOOL_ACTION_VERBS: &[&str] = &[
    "Actioning",
    "Actualizing",
    "Architecting",
    "Avenging",
    "Baking",
    "Batmanning",
    "Beaming",
    "Beboppin'",
    "Befuddling",
    "Billowing",
    "Blanching",
    "Bloviating",
    "Boogieing",
    "Boondoggling",
    "Booping",
    "Bootstrapping",
    "Brewing",
    "Bunning",
    "Burrowing",
    "Calculating",
    "Canoodling",
    "Caramelizing",
    "Cascading",
    "Catapulting",
    "Cerebrating",
    "Channeling",
    "Channelling",
    "Choreographing",
    "Churning",
    "Clauding",
    "Coalescing",
    "Cogitating",
    "Combobulating",
    "Composing",
    "Computing",
    "Concocting",
    "Considering",
    "Contemplating",
    "Cooking",
    "Crafting",
    "Creating",
    "Crunching",
    "Crystallizing",
    "Cultivating",
    "Deadpooling",
    "Deciphering",
    "Deliberating",
    "Demogorgon-ing",
    "Determining",
    "Dilly-dallying",
    "Discombobulating",
    "Doing",
    "Doodling",
    "Drizzling",
    "Dracarys-ing",
    "Dune-sandworming",
    "Ebbing",
    "Effecting",
    "Elucidating",
    "Embellishing",
    "Enchanting",
    "Envisioning",
    "Evaporating",
    "Expecto-patronuming",
    "Fermenting",
    "Fiddle-faddling",
    "Finagling",
    "Flambéing",
    "Flibbertigibbeting",
    "Flowing",
    "Flummoxing",
    "Fluttering",
    "Forging",
    "Forming",
    "Frolicking",
    "Frosting",
    "Gallivanting",
    "Galloping",
    "Garnishing",
    "Generating",
    "Gesticulating",
    "Germinating",
    "Gitifying",
    "Grokking",
    "Grooving",
    "Gusting",
    "Harmonizing",
    "Hashing",
    "Hatching",
    "Herding",
    "Hogwart-ing",
    "Honking",
    "Hulk-smashing",
    "Hullaballooing",
    "Hyperspacing",
    "Ideating",
    "Imagining",
    "Improvising",
    "Inceptioning",
    "Incubating",
    "Inferring",
    "Infusing",
    "Ionizing",
    "Iron-manning",
    "Jedi-mindtricking",
    "Jitterbugging",
    "Julienning",
    "Kneading",
    "Leavening",
    "Levitating",
    "Loki-mischiefing",
    "Lollygagging",
    "Mandalorian-ing",
    "Manifesting",
    "Marinating",
    "Matrix-dodging",
    "Meandering",
    "Metamorphosing",
    "Misting",
    "Moonwalking",
    "Moseying",
    "Mulling",
    "Multiversing",
    "Mustering",
    "Musing",
    "Naruto-running",
    "Nebulizing",
    "Nesting",
    "Newspapering",
    "Noodling",
    "Nucleating",
    "One-punching",
    "Orbiting",
    "Orchestrating",
    "Osmosing",
    "Perambulating",
    "Percolating",
    "Perusing",
    "Philosophising",
    "Photosynthesizing",
    "Pikachu-thundering",
    "Pollinating",
    "Pondering",
    "Pontificating",
    "Pouncing",
    "Precipitating",
    "Prestidigitating",
    "Processing",
    "Proofing",
    "Propagating",
    "Puttering",
    "Puzzling",
    "Quantumizing",
    "Razzle-dazzling",
    "Razzmatazzing",
    "Recombobulating",
    "Reticulating",
    "Rickrolling",
    "Roosting",
    "Ruminating",
    "Sautéing",
    "Scampering",
    "Schlepping",
    "Schruting",
    "Scurrying",
    "Seasoning",
    "Shenaniganing",
    "Sherlocking",
    "Shimmying",
    "Simmering",
    "Skedaddling",
    "Sketching",
    "Slithering",
    "Smooshing",
    "Sock-hopping",
    "Sonic-spindashing",
    "Spelunking",
    "Spinning",
    "Sprouting",
    "Stewing",
    "Stranger-things-ing",
    "Sublimating",
    "Super-saiyaning",
    "Swirling",
    "Swooping",
    "Symbioting",
    "Synthesizing",
    "TARDIS-ing",
    "Taylor-swifting",
    "Tempering",
    "Thanos-snapping",
    "Thinking",
    "Thundering",
    "Tinkering",
    "Titan-ing",
    "Tomfoolering",
    "Topsy-turvying",
    "Transfiguring",
    "Transmuting",
    "Twisting",
    "Undulating",
    "Unfurling",
    "Unravelling",
    "Vibing",
    "Waddling",
    "Wakanda-forevering",
    "Wandering",
    "Warping",
    "Whatchamacalliting",
    "Whirlpooling",
    "Whirring",
    "Whisking",
    "Wibbling",
    "Wingardium-leviosaing",
    "Working",
    "Wrangling",
    "Yeeting",
    "Zesting",
    "Zigzagging",
    "Among-us-ing",
    "Barbie-dreamhousing",
    "Bruno-ing",
    "Captaining",
    "Clicker-surviving",
    "Doctor-stranging",
    "Dumbledore-ing",
    "Elden-ring-ing",
    "Fortnite-dancing",
    "Goku-ing",
    "House-of-the-dragoning",
    "Interstellar-ing",
    "Kirby-inhaling",
    "Last-of-us-ing",
    "Leveling-up",
    "Mario-jumping",
    "Millennium-falconing",
    "Minecraft-crafting",
    "No-scoping",
    "Oppenheimer-ing",
    "Pokémon-masterballing",
    "Speedrunning",
    "Squid-gaming",
    "This-is-the-waying",
    "Walter-white-cooking",
    "Wednesday-adding",
    "Wolverine-adamantiuming",
];

/// Time bucket for cycling through verbs (milliseconds).
const LLM_PENDING_BUCKET_MS: u64 = 2200;

fn stable_hash(name: &str) -> usize {
    name.bytes().fold(0usize, |acc, b| {
        acc.wrapping_mul(131).wrapping_add(b as usize)
    })
}

pub fn shimmer_wrap(text: &str, phase: usize) -> String {
    let left = SHIMMER_GLYPHS[phase % SHIMMER_GLYPHS.len()];
    let right = SHIMMER_GLYPHS[(phase + 3) % SHIMMER_GLYPHS.len()];
    format!("{left} {text} {right}")
}

/// Get an LLM pending verb based on elapsed time and iteration.
pub fn glitter_verb_for_llm_pending(elapsed_ms: u128, iteration: u32) -> String {
    let bucket = elapsed_ms.saturating_div(LLM_PENDING_BUCKET_MS as u128) as usize;
    let round_offset = iteration.saturating_sub(1) * 5;
    let idx = (bucket * 7 + 3 + round_offset as usize) % TOOL_ACTION_VERBS.len();
    let verb = TOOL_ACTION_VERBS[idx];

    format!("{verb}...")
}

/// Get glitter verb specifically for a tool-call start event.
pub fn glitter_verb_for_tool_call(name: &str, call_index: u64, _shimmer_phase: usize) -> String {
    let seed = stable_hash(name).wrapping_add(call_index as usize * 17);
    let verb = TOOL_ACTION_VERBS[seed % TOOL_ACTION_VERBS.len()];
    format!("{verb}...")
}

/// Tool-specific glitter verbs.
fn glitter_verb_for_tool(name: &str) -> &'static str {
    match name {
        "read_file" => "Reading...",
        "write_file" => "Writing...",
        "delete_file" => "Deleting...",
        "list_directory" => "Listing...",
        "open_workspace" => "Opening workspace...",
        "canvas_list_modules" => "Scanning canvas...",
        "canvas_create_tile" => "Adding tiles...",
        "canvas_update_tile" => "Updating canvas...",
        _ => "Running tool...",
    }
}

/// Get glitter verb for multiple tools.
pub fn glitter_verb_for_tools(names: &[String]) -> String {
    let unique: Vec<&String> = names.iter().filter(|s| !s.is_empty()).collect();

    if unique.is_empty() {
        return "Working...".to_string();
    }

    if unique.len() == 1 {
        let name = unique[0];
        let verb = glitter_verb_for_tool(name);
        return verb.to_string();
    }

    // Check if all tools are the same
    let all_same = unique.iter().all(|x| **x == **unique[0]);
    if all_same {
        let base = glitter_verb_for_tool(unique[0]);
        let stripped = base.trim_end_matches("...");
        return format!("{stripped} (×{})...", unique.len());
    }

    format!("Running {} tools...", unique.len())
}

/// Get glitter verb for LLM round.
pub fn glitter_verb_for_llm(iteration: u32) -> &'static str {
    if iteration <= 1 {
        "Round 1 — calling model..."
    } else {
        "Thinking..."
    }
}

/// Calculate elapsed milliseconds since start.
pub fn elapsed_ms_since(start: Option<Instant>) -> u128 {
    match start {
        Some(s) => s.elapsed().as_millis(),
        None => 0,
    }
}

/// Get the appropriate glitter verb based on current state.
pub fn get_glitter_verb(
    is_running: bool,
    iteration: u32,
    active_tools: &[String],
    run_started_at: Option<Instant>,
) -> String {
    if !is_running {
        "ready".to_string()
    } else if !active_tools.is_empty() {
        glitter_verb_for_tools(active_tools)
    } else {
        let elapsed = elapsed_ms_since(run_started_at);
        glitter_verb_for_llm_pending(elapsed, iteration)
    }
}
