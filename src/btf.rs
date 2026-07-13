use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use btf_rs::{BtfType, Type, Union};

use crate::analyzer::semantic_analyzer::{FieldInfo, StructInfo, TypeInfo};
use crate::parser::TypeKind;

const VMLINUX: &str = "/sys/kernel/btf/vmlinux";
const MAX_CHAIN: usize = 8;
const BTF_PROBE_KINDS: &[&str] = &["fentry", "fexit", "kfunc", "kretfunc"];

pub fn probe_func<'a>(attach_points: &[&'a str]) -> Option<&'a str> {
    for ap in attach_points {
        let kind = ap.split(':').next().unwrap_or_default();
        if !BTF_PROBE_KINDS.contains(&kind) {
            continue;
        }
        if let Some(func) = ap.rsplit(':').next() {
            if !func.is_empty() {
                return Some(func);
            }
        }
    }
    None
}

#[derive(Debug, Default)]
pub struct BtfScope {
    pub args: StructInfo,
    pub structs: HashMap<String, StructInfo>,
}

// loading vmlinux costs tens of milliseconds, so we do it
// lazily and per function scopes are memoized
pub struct Btf {
    inner: OnceLock<Option<btf_rs::Btf>>,
    scopes: Mutex<HashMap<String, Arc<BtfScope>>>,
}

impl Btf {
    pub fn new() -> Self {
        Self {
            inner: OnceLock::new(),
            scopes: Mutex::new(HashMap::new()),
        }
    }

    fn load(&self) -> Option<&btf_rs::Btf> {
        self.inner
            .get_or_init(|| {
                let path =
                    std::env::var("BTF_VMLINUX_PATH").unwrap_or_else(|_| VMLINUX.to_string());
                btf_rs::Btf::from_file(path).ok()
            })
            .as_ref()
    }

    pub fn probe_scope(&self, func: &str) -> Option<Arc<BtfScope>> {
        if let Some(scope) = self.scopes.lock().unwrap().get(func) {
            return Some(scope.clone());
        }
        let scope = Arc::new(self.build_scope(func)?);
        self.scopes
            .lock()
            .unwrap()
            .insert(func.to_string(), scope.clone());
        Some(scope)
    }

    fn build_scope(&self, func: &str) -> Option<BtfScope> {
        let btf = self.load()?;
        let proto = self.func_prototype(btf, func)?;

        let mut structs = HashMap::new();
        let mut fields = Vec::new();
        for param in &proto.parameters {
            if param.is_variadic() {
                continue;
            }
            let name = btf.resolve_name(param).unwrap_or_default();
            if name.is_empty() {
                continue;
            }
            let type_name = resolve_type(btf, param.get_type_id(), &mut structs);
            fields.push(FieldInfo { name, type_name });
        }

        Some(BtfScope {
            args: StructInfo { fields },
            structs,
        })
    }

    fn func_prototype(&self, btf: &btf_rs::Btf, name: &str) -> Option<btf_rs::FuncProto> {
        let func = btf
            .resolve_types_by_name(name)
            .ok()?
            .into_iter()
            .find_map(|t| match t {
                Type::Func(f) => Some(f),
                _ => None,
            })?;
        match btf.resolve_chained_type(&func) {
            Ok(Type::FuncProto(proto)) => Some(proto),
            _ => None,
        }
    }
}

fn resolve_type(
    btf: &btf_rs::Btf,
    mut id: Option<u32>,
    structs: &mut HashMap<String, StructInfo>,
) -> TypeInfo {
    let mut pointers = 0;
    let mut typedef_name: Option<String> = None;

    for _ in 0..MAX_CHAIN {
        let Some(current) = id else { break };
        match btf.resolve_type_by_id(current) {
            Ok(Type::Ptr(p)) => {
                pointers += 1;
                id = p.get_type_id();
            }
            Ok(Type::Const(c)) | Ok(Type::Volatile(c)) | Ok(Type::Restrict(c)) => {
                id = c.get_type_id();
            }
            Ok(Type::Typedef(t)) => {
                if typedef_name.is_none() {
                    typedef_name = btf.resolve_name(&t).ok().filter(|n| !n.is_empty());
                }
                id = t.get_type_id();
            }
            Ok(Type::Struct(s)) => {
                let Ok(name) = btf.resolve_name(&s) else {
                    return TypeInfo::Builtin {
                        name: "void".into(),
                        pointers,
                    };
                };
                collect_struct(btf, &name, &s, structs);
                return TypeInfo::StructLike {
                    kind: TypeKind::Struct,
                    name,
                    pointers,
                };
            }
            Ok(Type::Union(u)) => {
                let Ok(name) = btf.resolve_name(&u) else {
                    return TypeInfo::Builtin {
                        name: "void".into(),
                        pointers,
                    };
                };
                collect_struct(btf, &name, &u, structs);
                return TypeInfo::StructLike {
                    kind: TypeKind::Union,
                    name,
                    pointers,
                };
            }
            Ok(other) => {
                let name = typedef_name
                    .or_else(|| other.as_btf_type().and_then(|t| btf.resolve_name(t).ok()))
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| other.name().to_string());
                return TypeInfo::Builtin { name, pointers };
            }
            Err(_) => break,
        }
    }
    TypeInfo::Builtin {
        name: typedef_name.unwrap_or_else(|| "void".into()),
        pointers,
    }
}

fn collect_struct(
    btf: &btf_rs::Btf,
    name: &str,
    r#struct: &Union,
    structs: &mut HashMap<String, StructInfo>,
) {
    if structs.contains_key(name) {
        return;
    }
    // reserve the slot first to avoid cyclic references
    structs.insert(name.to_string(), StructInfo::default());

    let mut fields = Vec::new();
    for member in &r#struct.members {
        let field_name = btf.resolve_name(member).unwrap_or_default();
        if field_name.is_empty() {
            continue;
        }
        let type_name = resolve_type(btf, member.get_type_id(), structs);
        fields.push(FieldInfo {
            name: field_name,
            type_name,
        });
    }
    structs.insert(name.to_string(), StructInfo { fields });
}
