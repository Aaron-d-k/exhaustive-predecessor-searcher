use std::fs;
use std::fs::File;
use std::io::Write;
use std::process;

mod basic_grids;
mod board;
mod ca;
mod format;
mod search;
mod stats;
use std::time::SystemTime;

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

use crate::Commands::DeepSearch;
use crate::basic_grids::Direction;
use crate::basic_grids::Directions;
use crate::basic_grids::Grid;
use crate::board::BoardWindow;
use crate::ca::CACell;
use crate::ca::RuleLut;
use crate::search::CombinerCfg;
use crate::stats::print_viz_fracs;

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[arg(short, long, default_value_t = 1, value_parser = clap::value_parser!(i32).range(-1..=2))]
    verbosity: i32,

    #[arg(short, long, value_name="FILE", default_value = concat!(env!("CARGO_MANIFEST_DIR"),"/b3s23.cfg"))]
    cfg_file: PathBuf,

    #[arg(long, value_enum, default_value_t = Mode::KeepLowCost)]
    cull_mode: Mode,

    #[arg(long, default_value_t = 0.1)]
    cullfrac: f64,

    #[command(subcommand)]
    command: Commands,
}

#[derive(ValueEnum, Clone, Debug)]
enum Mode {
    Exhaustive,
    KeepLowCost,
    KeepRandom,
}

#[derive(Subcommand)]
enum Commands {
    #[command(about="Tries to search for a n-gen predecessor, using the given heuristic (default: low population) to choose the best predecessor.")]
    DeepSearch {
        #[arg(short, long, default_value_t = 1)]
        gens: u32,

        #[arg(short, long, value_name = "FILE")]
        inputfile: PathBuf,

        #[arg(short, long, default_value_t = 3)]
        n_patterns_to_print_per_gen: u32,

        #[arg(short = 'l', long, default_value_t = 100000000)]
        limit_patterns_midlayer: usize,

        #[arg(short = 'f', long, default_value_t = 1000000)]
        limit_patterns_finallayer: usize,
    },
    #[command(about="Tries to search for every 1-gen predecessor, using the given heuristic to sort predecessors.")]
    ExhaustiveSearch {
        #[arg(short, long, value_name = "FILE")]
        inputfile: PathBuf,

        #[arg(short, long, value_name = "FILE")]
        templateoutput: Option<PathBuf>,

        #[arg(short, long, value_name = "FILE", default_value = "preds.txt")]
        outputfile: PathBuf,

        #[arg(short = 'l', long, default_value_t = 100000000)]
        limit_patterns_midlayer: usize,

        #[arg(short = 'f', long, default_value_t = 1000000)]
        limit_patterns_finallayer: usize,

        #[arg(long, default_value_t = false)]
        /// Makes the search faster by eliminating certain boards with duplicate behaviour. This makes the search non-exhaustive, but it will still never miss a solution as long as at least one exists.
        cull_high_pop: bool,
    },
}

pub fn parse_file_rle(f: &PathBuf) -> Grid<char> {
    let contents = fs::read_to_string(&f).unwrap_or_else(|err| {
        eprintln!("Error reading file '{}': {}", f.display(), err);
        process::exit(1);
    });

    format::parse_generic_rle(&contents).unwrap_or_else(|e| {
        eprintln!("Failed to parse pattern: {}", e);
        process::exit(1)
    })
}

fn main() {
    let args = Cli::parse();

    let rule_lut = RuleLut::from_cfg(&fs::read_to_string(&args.cfg_file).unwrap());

    match args.command {
        DeepSearch {
            gens,
            ref inputfile,
            n_patterns_to_print_per_gen,
            limit_patterns_midlayer,
            limit_patterns_finallayer,
        } => {
            let input_patt = parse_file_rle(inputfile).map(|x| match x {
                '.' | 'b' => CACell::DEAD,
                _ => CACell::ALIVE,
            });

            deep_main(
                &input_patt,
                &rule_lut,
                gens,
                n_patterns_to_print_per_gen,
                limit_patterns_midlayer,
                limit_patterns_finallayer,
                &args,
            );
        }
        Commands::ExhaustiveSearch {
            ref inputfile,
            ref templateoutput,
            ref outputfile,
            limit_patterns_midlayer,
            limit_patterns_finallayer,
            cull_high_pop,
        } => {
            let input_patt = parse_file_rle(inputfile).map(|x| match x {
                '.' | 'b' => Some(CACell::DEAD),
                'o' | 'A' => Some(CACell::ALIVE),
                _ => None,
            });

            let o_template = templateoutput
                .as_ref()
                .map(|x| {
                    parse_file_rle(x).map(|x| match x {
                        '.' | 'b' => Some(CACell::DEAD),
                        'o' | 'A' => Some(CACell::ALIVE),
                        _ => None,
                    })
                })
                .unwrap_or(Grid::from_rect(input_patt.size, None));

            exhaustive_main(
                &outputfile,
                &input_patt,
                &o_template,
                &rule_lut,
                limit_patterns_midlayer,
                limit_patterns_finallayer,
                cull_high_pop,
                &args,
            );
        }
    }
}

fn makecfg(plim: usize, args: &Cli) -> CombinerCfg {
    match args.cull_mode {
        Mode::Exhaustive => CombinerCfg::Exhaustive,
        Mode::KeepLowCost => CombinerCfg::KeepLowCost {
            patt_limit: plim,
            cullfrac: args.cullfrac,
        },
        Mode::KeepRandom => CombinerCfg::KeepRandom { patt_limit: plim },
    }
}

fn exhaustive_main(
    of: &PathBuf,
    input_patt: &Grid<Option<CACell>>,
    o_template: &Grid<Option<CACell>>,
    rule_lut: &RuleLut,
    limit_patterns_midlayer: usize,
    limit_patterns_finallayer: usize,
    cull_high_pop: bool,
    args: &Cli,
) {
    if args.verbosity > 0 {
        eprintln!(
            "Dimensions: {}x{}",
            input_patt.size.width(),
            input_patt.size.height()
        );
        eprintln!("\nParsed Grid:");
        let pfn = |x: &Option<CACell>| match *x {
            None => '?',
            Some(CACell::ALIVE) => '*',
            Some(CACell::DEAD) => '.',
        };
        eprintln!("{}", input_patt.to_plaintext_generic(pfn));
        eprintln!("{}", o_template.to_plaintext_generic(pfn));
    }
    let mut bw = BoardWindow::new(
        input_patt.size,
        Directions::all(),
        input_patt.size.height() as f32,
        input_patt.size.width() as f32,
    );
    bw.fill_leaves(&rule_lut, o_template, input_patt);

    for d in (1..=bw.min_leaf_depth()).rev() {
        if args.verbosity > 0 {eprintln!("Combining to depth {} ", d);}
        search::fill_combinations(
            &mut bw,
            d,
            &makecfg(limit_patterns_midlayer, args),
            args.verbosity,
        );

        if args.verbosity > 0 {eprintln!("Starting culling depth {d} ");}
        search::extract_and_cull(&mut bw, d, 0.01, args.verbosity).unwrap();
        bw.free_caches(d as i32);

        if cull_high_pop {
            search::extract_and_cull_lowcost(&mut bw, d, args.verbosity).unwrap()
        };
    }

    if args.verbosity > 0 {eprintln!("Finishing up...");}
    search::fill_combinations(
        &mut bw,
        0,
        &makecfg(limit_patterns_finallayer, args),
        args.verbosity,
    );
    let nboard = bw.get_num_valid_boards().unwrap();
    eprintln!("Number of solutions found: {nboard}");
    let mut of = File::create(of).unwrap();
    for i in 0..nboard {
        writeln!(
            of,
            "Solution found:\n{}",
            bw.extract_board(i).unwrap().to_plaintext()
        )
        .unwrap();
    }
}

fn deep_main(
    input_patt: &Grid<CACell>,
    rule_lut: &RuleLut,
    gens: u32,
    n_patterns_to_print_per_gen: u32,
    limit_patterns_midlayer: usize,
    limit_patterns_finallayer: usize,
    args: &Cli,
) {
    let mut gens_left = gens;
    let mut current_candidates = vec![input_patt.clone(); 1];

    let starttime = SystemTime::now();

    let mut input_num = 0;
    let mut border_size = 0;
    let mut start_side = Direction::Up;

    while gens_left > 0 {
        let mut curr_patt = current_candidates[input_num].clone();
        let mut curr_direc = start_side;
        for _ in 0..border_size {
            curr_patt = curr_patt.with_border_direction(1, curr_direc);
            curr_direc = curr_direc.rotate_90_cw();
        }
        let curr_patt = curr_patt;

        if args.verbosity > 1 {eprintln!(
            "Elapsed time: {}s, Gens Left: {gens_left}",
            SystemTime::now()
                .duration_since(starttime)
                .expect("Tenet")
                .as_secs()
        );
        eprintln!(
            "Dimensions: {}x{}",
            curr_patt.size.width(),
            curr_patt.size.height()
        );
        }
        if args.verbosity > 0 {eprintln!("\nParsed Grid:");
        eprintln!("{}", curr_patt.to_plaintext());}

        let mut bw = BoardWindow::new(
            curr_patt.size,
            Directions::all(),
            curr_patt.size.height() as f32,
            curr_patt.size.width() as f32,
        );
        bw.fill_leaves(
            &rule_lut,
            &Grid::from_rect(bw.rect, None),
            &curr_patt.map(|&x| Some(x)),
        );

        for d in (1..=bw.min_leaf_depth()).rev() {
            if args.verbosity > 0 { eprintln!("Combining to depth {} ", d);}
            search::fill_combinations(
                &mut bw,
                d,
                &makecfg(limit_patterns_midlayer, args),
                args.verbosity,
            );

            if args.verbosity > 0{eprintln!("Starting culling depth {d} ");}
            search::extract_and_cull(&mut bw, d, 0.01, args.verbosity).unwrap();
            bw.free_caches(d as i32);
            search::extract_and_cull_lowcost(&mut bw, d, args.verbosity).unwrap();
        }
        if args.verbosity > 0{eprintln!("Finishing up...");}
        search::fill_combinations(
            &mut bw,
            0,
            &makecfg(limit_patterns_finallayer, args),
            args.verbosity,
        );
        let nboard = bw.get_num_valid_boards().unwrap();
        if args.verbosity > -1 || gens_left==1 {eprintln!("Number of solutions found: {nboard}");}

        if nboard == 0 {
            if args.verbosity > 0 {eprintln!("Failed, trying bigger....");}
            input_num += 1;
            if input_num == current_candidates.len() {
                if start_side == Direction::Left || border_size % 4 == 0 {
                    border_size += 1;
                }
                start_side = start_side.rotate_90_cw();
                input_num = 0;
            }
        } else {
            let heatmap = bw.get_all_boards_stats().unwrap();
            if args.verbosity > -1 {println!("Printing all solutions heatmap:");
            print_viz_fracs(&heatmap);}
            if args.verbosity > -1 || gens_left==1 {for i in 0..bw
                .get_num_valid_boards()
                .unwrap()
                .min(n_patterns_to_print_per_gen as u64)
            {
                println!(
                    "Solution found:\n{}",
                    bw.extract_board(i).unwrap().to_plaintext()
                )
            }}
            bw.build_cost_cache();
            let index_of_best = top_k_indices(
                &(0..nboard)
                    .into_iter()
                    .map(|i| bw.get_cost(i).unwrap())
                    .collect::<Vec<_>>(),
                2,
            );
            current_candidates = index_of_best
                .iter()
                .map(|&i| bw.extract_board(i as u64).unwrap().without_border(1))
                .collect();
            current_candidates
                .sort_by_cached_key(|g| g.data.iter().map(|&c| c.into_u8() as u32).sum::<u32>());
            input_num = 0;
            border_size = 0;
            start_side = Direction::Up;
            gens_left -= 1;
        }
    }
}

fn top_k_indices<T: PartialOrd>(slice: &[T], k: usize) -> Vec<usize> {
    if slice.len() <= k {
        return (0..slice.len()).collect();
    }

    let mut indexed_elements: Vec<(usize, &T)> = slice.iter().enumerate().collect();

    indexed_elements.select_nth_unstable_by(k - 1, |a, b| {
        b.1.partial_cmp(a.1).unwrap_or(std::cmp::Ordering::Equal)
    });

    indexed_elements
        .iter()
        .take(k)
        .map(|&(idx, _)| idx)
        .collect()
}
