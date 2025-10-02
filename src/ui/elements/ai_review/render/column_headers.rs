// Column header construction for AI batch review UI
use egui_extras::{Table, TableBuilder};

#[allow(dead_code)]
pub struct ColumnHeaderContext<'a> {
    pub builder: TableBuilder<'a>,
}

#[allow(dead_code)]
impl<'a> ColumnHeaderContext<'a> {
    pub fn add_headers(
        self,
        ancestor_key_columns_len: usize,
        union_cols_len: usize,
        structure_cols_len: usize,
    ) -> Table<'a> {
        self.builder.header(22.0, move |mut header| {
            header.col(|_| {}); // control column
            for _ in 0..ancestor_key_columns_len {
                header.col(|ui| {
                    ui.label("Key");
                });
            }
            for _ in 0..union_cols_len {
                header.col(|ui| {
                    ui.label("Col");
                });
            }
            for _ in 0..structure_cols_len {
                header.col(|ui| {
                    ui.label("Struct");
                });
            }
        })
    }
}
