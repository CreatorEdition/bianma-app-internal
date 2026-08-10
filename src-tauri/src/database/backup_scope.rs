//! 设备本地数据库关系的备份边界。
//!
//! 本模块只声明哪些表永不进入便携备份，以及它们在当前设备恢复时的外键顺序。
//! 它不持有连接、不读取 Secret，也不参与路由或代理热路径。

/// 设备本地表的静态备份声明。
#[derive(Clone, Copy)]
pub(crate) struct LocalOnlyRelation {
    pub(crate) table: &'static str,
    pub(crate) restore_rank: u8,
}

const ROUTING_V2_LOCAL_ONLY_RELATIONS: &[LocalOnlyRelation] = &[
    LocalOnlyRelation {
        table: "routing_v2_store_state",
        restore_rank: 0,
    },
    LocalOnlyRelation {
        table: "routing_v2_sites",
        restore_rank: 1,
    },
    LocalOnlyRelation {
        table: "routing_v2_endpoints",
        restore_rank: 2,
    },
    LocalOnlyRelation {
        table: "routing_v2_accounts",
        restore_rank: 2,
    },
    LocalOnlyRelation {
        table: "routing_v2_model_deployments",
        restore_rank: 3,
    },
    LocalOnlyRelation {
        table: "routing_v2_migration_journal",
        restore_rank: 4,
    },
];

/// 返回所有已注册的设备本地表。
pub(crate) fn local_only_relations() -> &'static [LocalOnlyRelation] {
    ROUTING_V2_LOCAL_ONLY_RELATIONS
}

/// 按 SQLite 标识符的 ASCII 大小写无关规则查找设备本地表。
pub(crate) fn local_only_relation(table: &str) -> Option<LocalOnlyRelation> {
    ROUTING_V2_LOCAL_ONLY_RELATIONS
        .iter()
        .copied()
        .find(|relation| relation.table.eq_ignore_ascii_case(table))
}
