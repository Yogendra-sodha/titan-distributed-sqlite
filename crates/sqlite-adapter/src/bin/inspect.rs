use sqlite_adapter::SqliteAdapter;

fn main() {
    for i in 1..=3 {
        let path = format!("./data/titan_node_{}.db", i);
        println!("====== Node {} Database ======", i);
        if let Ok(adapter) = SqliteAdapter::new(&path) {
            match adapter.execute_read("SELECT * FROM demo;") {
                Ok(rows) => {
                    if rows.is_empty() {
                        println!("(empty table)");
                    } else {
                        for row in rows {
                            println!("{:?}", row);
                        }
                    }
                }
                Err(e) => println!("Table empty or uninitialized: {}", e),
            }
        } else {
            println!("DB file not found");
        }
        println!("");
    }
}
