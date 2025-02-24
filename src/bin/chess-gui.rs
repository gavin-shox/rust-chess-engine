// suppress console in Windows for release builds
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::env;
use std::ops::Deref;
use std::sync::{Arc, Mutex};

use env_logger::{Builder, Env, Target};
use slint::{ComponentHandle, SharedString};

use chess::fen::FEN;
use chess::pgn::PGN;
use chess::{eval_to_string, hash_to_string, PieceColour};

slint::include_modules!();

type PieceUI = slint_generatedBoard_UI::Piece_UI;
type PieceColourUI = slint_generatedBoard_UI::PieceColour_UI;
type PieceTypeUI = slint_generatedBoard_UI::PieceType_UI;
type MoveNotationUI = slint_generatedBoard_UI::MoveNotation_UI;
//type MoveUI = slint_generatedBoard_UI::Move_UI;

fn ui_convert_piece_colour(colour: chess::PieceColour) -> PieceColourUI {
    match colour {
        chess::PieceColour::White => PieceColourUI::White,
        chess::PieceColour::Black => PieceColourUI::Black,
    }
}

fn ui_convert_piece(piece: chess::Piece) -> PieceUI {
    let piece_colour = match piece.pcolour {
        chess::PieceColour::White => PieceColourUI::White,
        chess::PieceColour::Black => PieceColourUI::Black,
    };

    let piece_type = match piece.ptype {
        chess::PieceType::Pawn => PieceTypeUI::Pawn,
        chess::PieceType::Bishop => PieceTypeUI::Bishop,
        chess::PieceType::Knight => PieceTypeUI::Knight,
        chess::PieceType::Rook => PieceTypeUI::Rook,
        chess::PieceType::Queen => PieceTypeUI::Queen,
        chess::PieceType::King => PieceTypeUI::King,
    };

    PieceUI {
        piece_colour,
        piece_type,
    }
}

fn main() -> Result<(), slint::PlatformError> {
    // initialise logger
    let mut builder = if cfg!(debug_assertions) {
        Builder::from_env(Env::default().default_filter_or("debug"))
    } else {
        Builder::from_env(Env::default().default_filter_or("off"))
    };
    builder.target(Target::Stdout);
    builder.init();

    let board = Arc::new(Mutex::new(chess::Board::new()));

    let ui = Board_UI::new()?;
    let settings_dialog = SettingsDialog_UI::new()?;
    let import_dialog = Import_UI::new()?;
    let export_dialog = Export_UI::new()?;

    let ui_weak_get_gamestate = ui.as_weak();
    let board_get_gamestate = board.clone();
    ui.on_get_gamestate(move || {
        let ui = ui_weak_get_gamestate.upgrade().unwrap();
        let board = board_get_gamestate.lock().unwrap();
        let side_to_move = board.get_side_to_move();
        match board.get_game_over_state() {
            Some(state) => match state {
                chess::GameOverState::WhiteResign => {
                    ui.set_gamestate("White resigned".into());
                }
                chess::GameOverState::BlackResign => {
                    ui.set_gamestate("Black resigned".into());
                }
                chess::GameOverState::AgreedDraw => {
                    ui.set_gamestate("Draw agreed".into());
                }
                chess::GameOverState::Forced(gs) => {
                    if gs.is_win() {
                        ui.set_gamestate(format!("{} wins: {}", !side_to_move, gs).into());
                    } else {
                        ui.set_gamestate(format!("Draw: {}", gs).into());
                    }
                }
            },
            None => {
                ui.set_gamestate(
                    format!(
                        "{}'s turn: {}",
                        side_to_move,
                        board.get_current_state().get_gamestate()
                    )
                    .into(),
                );
            }
        }
    });

    let ui_weak_new_game = ui.as_weak();
    let board_new_game = board.clone();
    ui.on_new_game(move || {
        let ui = ui_weak_new_game.upgrade().unwrap();
        *board_new_game.lock().unwrap() = chess::board::Board::new();
        ui.invoke_refresh_position();
    });

    let ui_weak_new_chess960_game = ui.as_weak();
    let board_new_chess960_game = board.clone();
    ui.on_new_chess960_game(move || {
        let ui = ui_weak_new_chess960_game.upgrade().unwrap();
        *board_new_chess960_game.lock().unwrap() = chess::board::Board::new_chess960();
        ui.invoke_refresh_position();
    });

    let ui_select_legal_moves = ui.as_weak();
    let board_select_legal_moves = board.clone();
    ui.on_select_legal_moves(move |from_square| {
        let ui = ui_select_legal_moves.upgrade().unwrap();
        let board = board_select_legal_moves.lock().unwrap();
        let mut legal_moves = [false; 64];
        for mv in board.get_current_state().get_legal_moves().unwrap() {
            if mv.from as i32 == from_square {
                legal_moves[mv.to as usize] = true;
            }
        }
        ui.set_selected_legal_moves(
            std::rc::Rc::new(slint::VecModel::from(legal_moves.to_vec())).into(),
        );
    });

    let ui_weak_latest_state = ui.as_weak();
    let board_latest_state = board.clone();
    ui.on_latest_state(move || {
        let ui = ui_weak_latest_state.upgrade().unwrap();
        board_latest_state.lock().unwrap().checkout_latest_state();
        ui.set_selected_move_notation(
            board_latest_state
                .lock()
                .unwrap()
                .last_move_string_notation()
                .into(),
        );
        ui.set_detached_state(false);
        ui.invoke_refresh_position();
    });

    let ui_weak_prev_state = ui.as_weak();
    let board_prev_state = board.clone();
    ui.on_prev_state(move || {
        let ui = ui_weak_prev_state.upgrade().unwrap();
        let detatched = board_prev_state.lock().unwrap().checkout_prev();
        if detatched {
            ui.set_selected_move_notation(
                board_prev_state
                    .lock()
                    .unwrap()
                    .last_move_string_notation()
                    .into(),
            );
            ui.set_detached_state(true);
        } else {
            ui.set_selected_move_notation("".into());
            ui.set_detached_state(false)
        }
        let side = board_prev_state.lock().unwrap().get_side_to_move();
        ui.set_selected_halfmove(if side == PieceColour::White {
            2 // if white is to move, last halfmove was black
        } else {
            1 // last halfmove was white
        });
        ui.set_selected_move_number(if side == PieceColour::White {
            board_prev_state.lock().unwrap().get_current_move_count() as i32 - 1
        // if white is to move, last move was in movecount - 1
        } else {
            board_prev_state.lock().unwrap().get_current_move_count() as i32 // last halfmove is in current movecount
        });
        ui.invoke_refresh_position();
    });

    let ui_weak_next_state = ui.as_weak();
    let board_next_state = board.clone();
    ui.on_next_state(move || {
        let ui = ui_weak_next_state.upgrade().unwrap();
        let detatched = board_next_state.lock().unwrap().checkout_next();
        if detatched {
            ui.set_selected_move_notation(
                board_next_state
                    .lock()
                    .unwrap()
                    .last_move_string_notation()
                    .into(),
            );
            ui.set_detached_state(true);
        } else {
            ui.set_selected_move_notation("".into());
            ui.set_detached_state(false)
        }
        let side = board_next_state.lock().unwrap().get_side_to_move();
        ui.set_selected_halfmove(if side == PieceColour::White {
            2 // if white is to move, last halfmove was black
        } else {
            1 // last halfmove was white
        });
        ui.set_selected_move_number(if side == PieceColour::White {
            board_next_state.lock().unwrap().get_current_move_count() as i32 - 1
        // if white is to move, last move was in movecount - 1
        } else {
            board_next_state.lock().unwrap().get_current_move_count() as i32 // last halfmove is in current movecount
        });
        ui.invoke_refresh_position();
    });

    let ui_weak_find_state = ui.as_weak();
    let board_find_state = board.clone();
    ui.on_find_state(move |notation| {
        let ui = ui_weak_find_state.upgrade().unwrap();
        // get states that match notation and clone them into owned vector
        let states = board_find_state
            .lock()
            .unwrap()
            .find_states_by_notation(notation.as_str())
            .into_iter()
            .cloned()
            .collect::<Vec<chess::BoardState>>();

        let state = if states.len() > 1 {
            let selected_move_number = ui.get_selected_move_number();
            let selected_halfmove = ui.get_selected_halfmove();
            log::debug!("Multiple states found for notation: {}. Disambiguating based on selected move number: {} and selected halfmove: {}", 
                notation, selected_move_number, selected_halfmove);
            // find state with matching move number and halfmove
            states
                .iter()
                .find(|s| {
                    if s.side_to_move == PieceColour::White {
                        s.move_count() as i32 == selected_move_number + 1
                            && selected_halfmove == 2
                    } else {
                        s.move_count() as i32 == selected_move_number
                            && selected_halfmove == 1
                    }
                })
                .unwrap()
                .clone()
        } else {
            // guaranteed to have at least one state as notation is valid from gui
            log::debug!("Single state found for notation: {}", notation);
            states[0].clone()
        };

        // unwrap is safe as state was found
        board_find_state
            .lock()
            .unwrap()
            .checkout_state(&state)
            .unwrap();
        log::debug!("State checked out, detatched idx set");
        ui.set_detached_state(board_find_state.lock().unwrap().is_detatched());
        ui.invoke_refresh_position();
    });

    let ui_weak_refresh_position = ui.as_weak();
    let export_dialog_weak_refresh_position = export_dialog.as_weak();
    let board_refresh_position = board.clone();
    ui.on_refresh_position(move || {
        log::debug!("Refreshing position...");
        let ui = ui_weak_refresh_position.upgrade().unwrap();
        let export_dialog = export_dialog_weak_refresh_position.upgrade().unwrap();
        let mut ui_position: Vec<PieceUI> = vec![];
        for s in board_refresh_position
            .lock()
            .unwrap()
            .get_current_state()
            .get_pos64()
            .iter()
        {
            match s {
                chess::Square::Piece(p) => ui_position.push(ui_convert_piece(*p)),
                chess::Square::Empty => ui_position.push(PieceUI {
                    piece_colour: PieceColourUI::None,
                    piece_type: PieceTypeUI::None,
                }),
            }
        }
        // reverse board if player is black
        if ui.get_player_colour() == PieceColour_UI::Black {
            ui_position.reverse();
        }
        let pos = std::rc::Rc::new(slint::VecModel::from(ui_position));

        // generate move history as vector of move notations
        let ui_moves: Vec<SharedString> = board_refresh_position
            .lock()
            .unwrap()
            .move_history_string_notation()
            .iter()
            .map(|x| x.into())
            .collect();

        let mut ui_move_history: Vec<MoveNotationUI> = vec![];
        for (i, chunk) in ui_moves.chunks(2).enumerate() {
            let mut mv_notation = MoveNotationUI {
                move_number: (i + 1) as i32,
                notation1: "".into(),
                notation2: "".into(),
            };
            if chunk.len() == 2 {
                mv_notation.notation1 = chunk[0].clone();
                mv_notation.notation2 = chunk[1].clone();
            } else {
                mv_notation.notation1 = chunk[0].clone();
            }
            ui_move_history.push(mv_notation);
        }

        ui.set_move_history(std::rc::Rc::new(slint::VecModel::from(ui_move_history)).into());

        // set gamestate
        ui.invoke_get_gamestate();

        // set current BoardState FEN
        export_dialog.set_fen(
            FEN::from(board_refresh_position.lock().unwrap().get_current_state())
                .to_string()
                .into(),
        );
        log::debug!(
            "FEN: {} generated from boardstate with hash: {}",
            export_dialog.get_fen(),
            hash_to_string(
                board_refresh_position
                    .lock()
                    .unwrap()
                    .get_current_state()
                    .board_hash
            )
        );
        log::debug!(
            "Current position hash: {}",
            hash_to_string(
                board_refresh_position
                    .lock()
                    .unwrap()
                    .get_current_state()
                    .position_hash
            )
        );

        export_dialog.set_pgn(
            PGN::from(board_refresh_position.lock().unwrap().deref())
                .to_string()
                .into(),
        );
        log::debug!(
            "PGN generated from board with current boardstate hash: {}",
            hash_to_string(
                board_refresh_position
                    .lock()
                    .unwrap()
                    .get_current_state()
                    .board_hash
            )
        );

        if let Some(last_move) = board_refresh_position
            .lock()
            .unwrap()
            .get_current_state()
            .last_move
        {
            if ui.get_player_colour() == PieceColour_UI::Black {
                // reverse index if player is black as the board is flipped
                ui.set_last_move(Move_UI {
                    from_square: 63 - last_move.from as i32,
                    to_square: 63 - last_move.to as i32,
                });
            } else {
                ui.set_last_move(Move_UI {
                    from_square: last_move.from as i32,
                    to_square: last_move.to as i32,
                });
            }
        } else {
            ui.set_last_move(Move_UI {
                from_square: -1,
                to_square: -1,
            });
        }
        // set notation of last move as well
        ui.set_selected_move_notation(
            board_refresh_position
                .lock()
                .unwrap()
                .last_move_string_notation()
                .into(),
        );
        let side = board_refresh_position.lock().unwrap().get_side_to_move();
        ui.set_selected_halfmove(if side == PieceColour::White {
            2 // if white is to move, last halfmove was black
        } else {
            1 // last halfmove was white
        });
        ui.set_selected_move_number(if side == PieceColour::White {
            board_refresh_position
                .lock()
                .unwrap()
                .get_current_move_count() as i32
                - 1 // if white is to move, last move was in movecount - 1
        } else {
            board_refresh_position
                .lock()
                .unwrap()
                .get_current_move_count() as i32 // last halfmove is in current movecount
        });
        ui.set_position(pos.into());
        log::debug!("Position refreshed");
    });

    let ui_weak_make_move = ui.as_weak();
    let board_make_move = board.clone();
    ui.on_make_move(move || -> bool {
        let ui = ui_weak_make_move.upgrade().unwrap();

        let from = ui.get_selected_from_square();
        let to = ui.get_selected_to_square();
        let mut legal_mv: chess::Move = chess::NULL_MOVE;

        for mv in board_make_move
            .lock()
            .unwrap()
            .get_current_state()
            .get_legal_moves()
            .unwrap()
        // unwrap is safe as we are not using lazy legal move generation
        {
            // ui indexes are reversed if player is black
            if ui.get_player_colour() == PieceColour_UI::Black {
                if mv.from as i32 == 63 - from && mv.to as i32 == 63 - to {
                    legal_mv = *mv;
                }
            } else if mv.from as i32 == from && mv.to as i32 == to {
                legal_mv = *mv;
            }
        }
        // make move and return true if successful
        board_make_move.lock().unwrap().make_move(&legal_mv).is_ok()
    });

    let ui_weak_engine_make_move = ui.as_weak();
    let board_engine_make_move = board.clone();
    ui.on_engine_make_move(move || {
        let ui = ui_weak_engine_make_move.clone();
        let bmem: Arc<Mutex<chess::Board>> = board_engine_make_move.clone();
        let depth = ui
            .upgrade()
            .unwrap()
            .get_depth()
            .to_string()
            .parse::<i32>()
            .unwrap();
        std::thread::spawn(
            move || match bmem.lock().unwrap().make_engine_move(depth as u8) {
                Ok((_, eval)) => {
                    slint::invoke_from_event_loop(move || {
                        ui.upgrade().unwrap().invoke_refresh_position();
                        ui.upgrade().unwrap().set_engine_made_move(true);
                        ui.upgrade().unwrap().set_eval(eval_to_string(eval).into())
                    })
                    .unwrap();
                }
                Err(e) => {
                    log::error!("BoardStateError on making engine move: {e}");
                }
            },
        );
    });

    let import_dialog_weak_run = import_dialog.as_weak();
    ui.on_import_dialog(move || {
        let import_dialog = import_dialog_weak_run.upgrade().unwrap();
        import_dialog.show().unwrap();
    });

    let export_dialog_weak_run = export_dialog.as_weak();
    ui.on_export_dialog(move || {
        export_dialog_weak_run.upgrade().unwrap().show().unwrap();
    });

    // close all child dialogs/windows on main window close
    let import_dialog_weak_close = import_dialog.as_weak();
    let export_dialog_weak_close = export_dialog.as_weak();
    let settings_dialog_weak_close = settings_dialog.as_weak();
    ui.window()
        .on_close_requested(move || -> slint::CloseRequestResponse {
            let import_dialog = import_dialog_weak_close.upgrade().unwrap();
            let export_dialog = export_dialog_weak_close.upgrade().unwrap();
            let settings_dialog = settings_dialog_weak_close.upgrade().unwrap();
            import_dialog.hide().unwrap();
            settings_dialog.hide().unwrap();
            export_dialog.hide().unwrap();
            slint::CloseRequestResponse::HideWindow
        });

    let ui_weak_import_fen = ui.as_weak();
    let import_dialog_weak_import_fen = import_dialog.as_weak();
    let board_import_fen = board.clone();
    import_dialog.on_import_fen(move |fen: SharedString| {
        let import_dialog = import_dialog_weak_import_fen.upgrade().unwrap();
        let ui = ui_weak_import_fen.upgrade().unwrap();

        let new_board = chess::board::Board::from(match fen.parse::<FEN>() {
            Ok(f) => {
                import_dialog.set_fen_error(false);
                import_dialog.set_fen_str("".into());
                f
            }
            Err(e) => {
                import_dialog.set_fen_error(true);
                import_dialog.set_fen_error_message(e.to_string().into());
                return;
            }
        });

        let side_to_move = ui_convert_piece_colour(new_board.get_current_state().side_to_move);
        let player_side = if import_dialog.get_as_white() {
            PieceColour_UI::White
        } else {
            PieceColour_UI::Black
        };

        *board_import_fen.lock().unwrap() = new_board;

        ui.invoke_reset_properties(player_side, side_to_move);
        ui.invoke_refresh_position();
        import_dialog.hide().unwrap();
    });

    let import_dialog_weak_close = import_dialog.as_weak();
    import_dialog.on_close(move || {
        let import_dialog = import_dialog_weak_close.upgrade().unwrap();
        import_dialog.set_fen_error(false);
        import_dialog.set_fen_error_message("".into());
        import_dialog.set_fen_str("".into());
        import_dialog.set_pgn_error(false);
        import_dialog.set_pgn_error_message("".into());
        import_dialog.set_pgn_str("".into());
        import_dialog.hide().unwrap();
    });

    // on close window, invoke on_close to reset state
    let import_dialog_weak_close_requested = import_dialog.as_weak();
    import_dialog
        .window()
        .on_close_requested(move || -> slint::CloseRequestResponse {
            let import_dialog = import_dialog_weak_close_requested.upgrade().unwrap();
            import_dialog.invoke_close();
            slint::CloseRequestResponse::HideWindow
        });

    let ui_weak_import_pgn = ui.as_weak();
    let import_dialog_weak_import_pgn = import_dialog.as_weak();
    let board_import_pgn = board.clone();
    import_dialog.on_import_pgn(move |pgn: SharedString| {
        let import_dialog = import_dialog_weak_import_pgn.upgrade().unwrap();
        let ui = ui_weak_import_pgn.upgrade().unwrap();

        log::debug!("Importing PGN: \n{}", pgn);

        let pgn_import = pgn.as_str().parse::<PGN>();
        match pgn_import {
            Ok(p) => {
                log::debug!("Successfully parsed PGN");
                let new_board = chess::Board::try_from(p);
                match new_board {
                    Ok(b) => {
                        log::debug!("Successfully created board from PGN");
                        import_dialog.set_pgn_error(false);
                        import_dialog.set_pgn_error_message("".into());
                        log::debug!("Resetting UI properties and refreshing position");
                        let side = b.get_side_to_move();
                        *board_import_pgn.lock().unwrap() = b;
                        // TODO for now set both to sidetomove so engine doesnt make move
                        ui.invoke_reset_properties(
                            ui_convert_piece_colour(side),
                            ui_convert_piece_colour(side),
                        );
                        ui.invoke_refresh_position();
                        import_dialog.set_pgn_str("".into());
                        import_dialog.hide().unwrap();
                    }
                    Err(e) => {
                        log::error!("Error creating board from PGN: {}", e);
                        import_dialog.set_pgn_error(true);
                        import_dialog.set_pgn_error_message(e.to_string().into());
                    }
                }
            }
            Err(e) => {
                log::error!("Error parsing PGN: {}", e);
                import_dialog.set_pgn_error(true);
                import_dialog.set_pgn_error_message(e.to_string().into());
                // return
            }
        }
    });

    let import_dialog_weak_file = import_dialog.as_weak();
    import_dialog.on_get_file(move || -> SharedString {
        let import_dialog = import_dialog_weak_file.upgrade().unwrap();
        let path = match native_dialog::FileDialog::new()
            .set_location("~/Desktop")
            .add_filter("PGN File", &["pgn"])
            .show_open_single_file()
        {
            Ok(p) => match p {
                Some(path) => path,
                None => {
                    log::warn!("No file selected");
                    return "".into();
                }
            },
            Err(e) => {
                log::error!("Error opening file dialog: {}", e);
                import_dialog.set_pgn_error(true);
                import_dialog.set_pgn_error_message(e.to_string().into());
                return "".into();
            }
        };

        match std::fs::read_to_string(&path) {
            Ok(p) => {
                // clear error state on successful file read
                import_dialog.set_pgn_error(false);
                import_dialog.set_pgn_error_message("".into());
                p.into()
            }
            Err(e) => {
                log::error!("Error reading file: {}", e);
                import_dialog.set_pgn_error(true);
                import_dialog.set_pgn_error_message(e.to_string().into());
                "".into()
            }
        }
    });

    let export_dialog_weak_close = export_dialog.as_weak();
    export_dialog.on_close(move || {
        let export_dialog = export_dialog_weak_close.upgrade().unwrap();
        export_dialog.hide().unwrap();
    });

    let settings_dialog_weak_run = settings_dialog.as_weak();
    ui.on_settings_dialog(move || {
        let settings_dialog = settings_dialog_weak_run.upgrade().unwrap();
        settings_dialog.show().unwrap();
    });

    let ui_weak_set_theme = ui.as_weak();
    settings_dialog.on_set_theme(move |theme| {
        let ui = ui_weak_set_theme.upgrade().unwrap();
        ui.set_board_theme(theme);
    });

    let ui_weak_set_depth = ui.as_weak();
    settings_dialog.on_set_depth(move |depth| {
        let ui = ui_weak_set_depth.upgrade().unwrap();
        ui.set_depth(depth);
    });

    let ui_weak_set_piece_theme = ui.as_weak();
    settings_dialog.on_set_piece_theme(move |theme| {
        let ui = ui_weak_set_piece_theme.upgrade().unwrap();
        ui.set_piece_theme(theme);
    });

    let settings_dialog_weak_close = settings_dialog.as_weak();
    settings_dialog.on_close(move || {
        let settings_dialog = settings_dialog_weak_close.upgrade().unwrap();
        settings_dialog.hide().unwrap();
    });

    let ui_weak_set_show_eval = ui.as_weak();
    settings_dialog.on_set_show_eval(move |show| {
        let ui = ui_weak_set_show_eval.upgrade().unwrap();
        ui.set_show_eval(show);
    });

    let ui_weak_set_show_legal_moves = ui.as_weak();
    settings_dialog.on_set_show_legal_moves(move |show| {
        let ui = ui_weak_set_show_legal_moves.upgrade().unwrap();
        ui.set_show_legal_moves(show);
    });

    let ui_weak_set_show_last_move = ui.as_weak();
    settings_dialog.on_set_show_last_move(move |show| {
        let ui = ui_weak_set_show_last_move.upgrade().unwrap();
        ui.set_show_last_move(show);
    });

    ui.invoke_refresh_position();
    ui.run()
}
