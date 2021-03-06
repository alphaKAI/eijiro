use anyhow::{anyhow, ensure, Result};
use clap::{App, Arg, ArgMatches, SubCommand};
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use eijiro_parser::{fst, Dict};
use fst::{IntoStreamer, Streamer};

use log::{error, info, warn};

fn printer(key: &str, field: &eijiro_parser::Field) -> String {
    let header = match field.ident.as_ref() {
        Some(head) => format!("{{{}}} : ", head),
        None => "".to_string(),
    };

    format!(
        "{}{}{}{}",
        header,
        field.explanation.body,
        field
            .explanation
            .complements
            .iter()
            .fold("".to_string(), |mut p, c| {
                p += &format!("◆{}", c.body);
                p
            }),
        field.examples.iter().fold("".to_string(), |mut p, e| {
            p += &format!("\n        {}", e.sentence);
            p
        })
    )
}

const default_lookup_distance: u32 = 0;

fn lookup_word(word: &str, dict: &Dict) {
    println!("<Search word: [{}]>", word);
    let matcher = fst::automaton::Levenshtein::new(word, default_lookup_distance).unwrap();
    let mut stream = dict.keys.search(&matcher).into_stream();
    while let Some((k, idx)) = stream.next() {
        let item = std::str::from_utf8(k).unwrap();
        for f in &dict.fields[idx as usize] {
            println!("{}", printer(item, f));
        }
    }
}

fn cli_frontend(matches: ArgMatches, dict: Dict) {
    match matches.value_of("word") {
        Some(word) => lookup_word(&word, &dict),
        None => loop {
            let mut word = String::new();
            print!("=> ");
            std::io::stdout().flush().unwrap();
            std::io::stdin().read_line(&mut word).unwrap();
            let word = word.trim_end();
            if word == ":exit" {
                break;
            }
            lookup_word(&word, &dict);
        },
    }
}

fn gui_frontend(dict: Dict) {
    use gio::prelude::*;
    use glib::{Type, Value};
    use gtk::prelude::*;
    use gtk::{
        Application, Builder, CellRendererText, Entry, ListStore, TextView, TreeView,
        TreeViewColumn, Window,
    };

    let app = Application::new(Some("info.alpha-kai-net.eijiro"), Default::default())
        .expect("Failed to initialize GTK application");
    //let glade_file_path = "eijiro.glade";
    let dict = Rc::new(dict);
    app.connect_activate(move |app| {
        let builder = Builder::from_string(include_str!("../eijiro.glade"));
        let window = builder
            .get_object::<Window>("window")
            .expect("Failed to get handle of window");
        window.connect_delete_event(|_, _| {
            gtk::main_quit();
            Inhibit(false)
        });

        fn append_word(word: &str, word_list_store: &ListStore, word_column_id: u32) {
            let iter = word_list_store.insert(-1);
            word_list_store.set_value(&iter, word_column_id, &word.to_value() as &Value);
        }

        // Setup word_list
        let word_list = builder
            .get_object::<TreeView>("word_list")
            .expect("Failed to get handle of word_list");
        let (word_list_store, word_column_id) = {
            // Setup TreeView
            // TreeView Element Types
            let column_types = [Type::String];
            let word_list_store = ListStore::new(&column_types);
            // Setup first column
            let word_column = TreeViewColumn::new();
            let word_column_id: u32 = 0;
            // Initialize & config column
            {
                word_column.set_title("Word");
                let cell_renderer = CellRendererText::new();
                word_column.pack_start(&cell_renderer, true);
                word_column.add_attribute(&cell_renderer, "text", word_column_id as i32);
                word_list.append_column(&word_column);
            }
            word_list.set_model(Some(&word_list_store));

            (word_list_store, word_column_id)
        };
        word_list.set_activate_on_single_click(true);

        let word_entry = builder
            .get_object::<Entry>("word_entry")
            .expect("Failed to get handle of word_entry");

        let word_desc = builder
            .get_object::<TextView>("word_desc")
            .expect("Failed to get handle of word_desc");

        {
            let dict = dict.clone();
            word_entry.connect_key_release_event(move |word_entry, key_event| {
                word_list_store.clear();
                word_desc.get_buffer().unwrap().set_text(&"");

                let query = word_entry.get_buffer().get_text();
                if query == "" {
                    // nothing to do
                    return Inhibit(false);
                }

                let matcher = fst::automaton::Levenshtein::new(&query, 1).unwrap();
                let mut stream = dict.keys.search(&matcher).into_stream();

                let mut word_descs = vec![];
                while let Some((k, idx)) = stream.next() {
                    let item = std::str::from_utf8(k).unwrap();
                    let mut desc = "".to_string();
                    for f in &dict.fields[idx as usize] {
                        desc += &printer(item, f);
                        desc += "\n";
                    }
                    word_descs.push((String::from(item), desc));
                }

                let mut prefix_ok = vec![];
                let mut prefix_ng = vec![];

                for (word, desc) in word_descs.iter() {
                    let tp = (word.clone(), desc.clone());
                    if word.starts_with(&query) {
                        prefix_ok.push(tp);
                    } else {
                        prefix_ng.push(tp);
                    }
                }

                prefix_ok.append(&mut prefix_ng);

                let mut words = vec![];
                let mut descs = vec![];
                for (word, desc) in prefix_ok {
                    append_word(&word, &word_list_store, word_column_id);
                    words.push(word);
                    descs.push(desc);
                }

                if descs.len() > 0 {
                    word_desc.get_buffer().unwrap().set_text(&descs[0]);
                }

                let descs = descs.clone();
                let word_desc = word_desc.clone();
                word_list.connect_row_activated(move |_, tp, _| {
                    let empty = "".to_string();
                    let text = match tp.get_indices().iter().nth(0) {
                        Some(idx) => &descs[*idx as usize],
                        None => &empty,
                    };

                    word_desc.get_buffer().unwrap().set_text(text);
                });

                Inhibit(false)
            });
        }

        window.show_all();
    });

    app.run(&[]);
    gtk::main();
}

fn main() {
    pretty_env_logger::init();
    let app = App::new("eijiro-rs")
        .version("0.1.1 Forked")
        .author("algon-320 <algon.0320@mail.com>")
        .author("Akihiro Shoji <alpha.kai.net@alpha-kai-net.info>")
        .about("English-Japanese dictionary (using Eijiro)")
        .arg(Arg::with_name("word").required(false))
        .arg(
            Arg::with_name("gui_flag")
                .help("gui frontend frag")
                .short("g")
                .long("gui")
                .required(false),
        );
    let matches = app.get_matches();

    let dict = match std::fs::read("./dict_dump.bincode") {
        Ok(bytes) => {
            info!("Loading dict");
            let dict = bincode::deserialize(&bytes).unwrap();
            info!("Loaded dict");
            dict
        }
        Err(_) => {
            info!("Parse EIJIRO.txt");
            let dict_str = std::fs::read_to_string("./EIJIRO.txt").unwrap();
            let dict = eijiro_parser::parse(dict_str.as_str()).unwrap();
            let _ = std::fs::write("./dict_dump.bincode", bincode::serialize(&dict).unwrap());
            dict
        }
    };

    if matches.is_present("gui_flag") {
        gui_frontend(dict);
    } else {
        cli_frontend(matches, dict);
    }
}
