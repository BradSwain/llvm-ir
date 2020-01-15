use crate::constant::Constant;
use crate::function::{Function, FunctionAttribute, GroupID};
use crate::metadata::{MetadataNode, MetadataNodeID, MetadataNodeMap, MetadataRef};
use crate::name::Name;
use crate::types::{Type, Typed};
use log::debug;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};

/// See [LLVM 9 docs on Module Structure](https://releases.llvm.org/9.0.0/docs/LangRef.html#module-structure)
#[derive(Clone, Debug)]
pub struct Module {
    /// The name of the module
    pub name: String,
    /// See [LLVM 9 docs on Source Filename](https://releases.llvm.org/9.0.0/docs/LangRef.html#source-filename)
    pub source_file_name: String,
    /// See [LLVM 9 docs on Data Layout](https://releases.llvm.org/9.0.0/docs/LangRef.html#data-layout)
    pub data_layout: String, // llvm-hs parses this String into Option<DataLayout> with a custom parser
    /// See [LLVM 9 docs on Target Triple](https://releases.llvm.org/9.0.0/docs/LangRef.html#target-triple)
    pub target_triple: Option<String>,
    /// Functions which are defined (not just declared) in this `Module`.
    /// See [LLVM 9 docs on Functions](https://releases.llvm.org/9.0.0/docs/LangRef.html#functions)
    pub functions: Vec<Function>,
    /// See [LLVM 9 docs on Global Variables](https://releases.llvm.org/9.0.0/docs/LangRef.html#global-variables)
    pub global_vars: Vec<GlobalVariable>,
    /// See [LLVM 9 docs on Global Aliases](https://releases.llvm.org/9.0.0/docs/LangRef.html#aliases)
    pub global_aliases: Vec<GlobalAlias>,
    /// Structure types can be "identified", meaning named. This map holds the named structure types in this `Module`.
    /// See [LLVM 9 docs on Structure Type](https://releases.llvm.org/9.0.0/docs/LangRef.html#structure-type).
    /// A `None` value indicates an opaque type; see [LLVM 9 docs on Opaque Structure Types](https://releases.llvm.org/9.0.0/docs/LangRef.html#t-opaque).
    /// Note that this map is from struct name to `Type::StructType` variant, not to `Type::NamedStructType` variant (which would be redundant).
    ///
    /// `Arc<RwLock<_>>` is used rather than `Rc<RefCell<_>>` so that `Module` can remain `Sync`.
    /// This is important because it allows multiple threads to simultaneously access a
    /// single (immutable) `Module`.
    pub named_struct_types: HashMap<String, Option<Arc<RwLock<Type>>>>,
    // --TODO not yet implemented-- pub function_attribute_groups: Vec<FunctionAttributeGroup>,
    /// See [LLVM 9 docs on Module-Level Inline Assembly](https://releases.llvm.org/9.0.0/docs/LangRef.html#moduleasm)
    pub inline_assembly: String,
    /// See [LLVM 9 docs on Metadata](https://releases.llvm.org/9.0.0/docs/LangRef.html#metadata)
    pub metadata_nodes: MetadataNodeMap,
    /// See [LLVM 9 docs on Named Metadata](https://releases.llvm.org/9.0.0/docs/LangRef.html#namedmetadatastructure)
    pub named_metadatas: Vec<NamedMetadata>,
    // See [LLVM 9 docs on Comdats](https://releases.llvm.org/9.0.0/docs/LangRef.html#comdats)
    // pub comdats: Vec<Comdat>,  // Comdats appear to not be exposed in the LLVM C API, only the C++ API
}

impl Module {
    /// Get the `Function` having the given `Name` (if any).
    /// Note that `Function`s are named with `String`s and not `Name`s.
    pub fn get_func_by_name(&self, name: &str) -> Option<&Function> {
        self.functions.iter().find(|func| func.name == name)
    }

    /// Parse the LLVM bitcode (.bc) file at the given path to create a `Module`
    pub fn from_bc_path(path: impl AsRef<Path>) -> Result<Self, String> {
        // implementation here inspired by the `inkwell` crate's `Module::parse_bitcode_from_path`
        use std::ffi::{CStr, CString};
        use std::mem;

        let path = CString::new(
            path.as_ref()
                .to_str()
                .expect("Did not find a valid Unicode path string"),
        )
        .expect("Failed to convert to CString");
        debug!("Creating a Module from path {:?}", path);

        let memory_buffer = unsafe {
            let mut memory_buffer = std::ptr::null_mut();
            let mut err_string: *mut i8 = std::mem::zeroed();
            let return_code = LLVMCreateMemoryBufferWithContentsOfFile(
                path.as_ptr() as *const i8,
                &mut memory_buffer,
                &mut err_string,
            );
            if return_code != 0 {
                return Err(CStr::from_ptr(err_string)
                    .to_str()
                    .expect("Failed to convert CStr")
                    .to_owned());
            }
            memory_buffer
        };
        debug!("Created a MemoryBuffer");

        let context = crate::from_llvm::Context::new();

        use llvm_sys::bit_reader::LLVMParseBitcodeInContext2;
        let module = unsafe {
            let mut module: mem::MaybeUninit<LLVMModuleRef> = mem::MaybeUninit::uninit();
            let return_code =
                LLVMParseBitcodeInContext2(context.ctx, memory_buffer, module.as_mut_ptr());
            if return_code != 0 {
                return Err("Failed to parse bitcode".to_string());
            }
            module.assume_init()
        };
        debug!("Parsed bitcode to llvm_sys module");
        Ok(Self::from_llvm_ref(module))
    }
}

/// See [LLVM 9 docs on Global Variables](https://releases.llvm.org/9.0.0/docs/LangRef.html#global-variables)
#[derive(PartialEq, Clone, Debug)]
pub struct GlobalVariable {
    pub name: Name,
    pub linkage: Linkage,
    pub visibility: Visibility,
    pub is_constant: bool,
    pub ty: Type,
    pub addr_space: AddrSpace,
    pub dll_storage_class: DLLStorageClass,
    pub thread_local_mode: ThreadLocalMode,
    pub unnamed_addr: Option<UnnamedAddr>,
    pub initializer: Option<Constant>,
    pub section: Option<String>,
    pub comdat: Option<Comdat>, // llvm-hs-pure has Option<String> for some reason
    pub alignment: u32,
    // --TODO not yet implemented-- pub metadata: Vec<(String, MetadataRef<MetadataNode>)>,
}

impl Typed for GlobalVariable {
    fn get_type(&self) -> Type {
        self.ty.clone()
    }
}

/// See [LLVM 9 docs on Global Aliases](https://releases.llvm.org/9.0.0/docs/LangRef.html#aliases)
#[derive(PartialEq, Clone, Debug)]
pub struct GlobalAlias {
    pub name: Name,
    pub aliasee: Constant,
    pub linkage: Linkage,
    pub visibility: Visibility,
    pub ty: Type,
    pub addr_space: AddrSpace,
    pub dll_storage_class: DLLStorageClass,
    pub thread_local_mode: ThreadLocalMode,
    pub unnamed_addr: Option<UnnamedAddr>,
}

impl Typed for GlobalAlias {
    fn get_type(&self) -> Type {
        self.ty.clone()
    }
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum UnnamedAddr {
    Local,
    Global,
}

/// See [LLVM 9 docs on Linkage Types](https://releases.llvm.org/9.0.0/docs/LangRef.html#linkage)
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum Linkage {
    Private,
    Internal,
    External,
    ExternalWeak,
    AvailableExternally,
    LinkOnceAny,
    LinkOnceODR,
    LinkOnceODRAutoHide,
    WeakAny,
    WeakODR,
    Common,
    Appending,
    DLLImport,
    DLLExport,
    Ghost,
    LinkerPrivate,
    LinkerPrivateWeak,
}

/// See [LLVM 9 docs on Visibility Styles](https://releases.llvm.org/9.0.0/docs/LangRef.html#visibility-styles)
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum Visibility {
    Default,
    Hidden,
    Protected,
}

/// See [LLVM 9 docs on DLL Storage Classes](https://releases.llvm.org/9.0.0/docs/LangRef.html#dllstorageclass)
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum DLLStorageClass {
    Default,
    Import,
    Export,
}

/// See [LLVM 9 docs on Thread Local Storage Models](https://releases.llvm.org/9.0.0/docs/LangRef.html#thread-local-storage-models)
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum ThreadLocalMode {
    NotThreadLocal,
    GeneralDynamic,
    LocalDynamic,
    InitialExec,
    LocalExec,
}

/// For discussion of address spaces, see [LLVM 9 docs on Pointer Type](https://releases.llvm.org/9.0.0/docs/LangRef.html#pointer-type)
pub type AddrSpace = u32;

/// See [LLVM 9 docs on Attribute Groups](https://releases.llvm.org/9.0.0/docs/LangRef.html#attribute-groups)
#[derive(PartialEq, Eq, Clone, Debug)]
pub struct FunctionAttributeGroup {
    pub group_id: GroupID,
    pub attrs: Vec<FunctionAttribute>,
}

/// See [LLVM 9 docs on Named Metadata](https://releases.llvm.org/9.0.0/docs/LangRef.html#named-metadata)
#[derive(PartialEq, Eq, Clone, Debug)]
pub struct NamedMetadata {
    pub name: String,
    pub node_ids: Vec<MetadataNodeID>,
}

/// See [LLVM 9 docs on Comdats](https://releases.llvm.org/9.0.0/docs/LangRef.html#langref-comdats)
#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Comdat {
    pub name: String,
    pub selection_kind: SelectionKind,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum SelectionKind {
    Any,
    ExactMatch,
    Largest,
    NoDuplicates,
    SameSize,
}

/* llvm-hs parses the data_layout into basically this structure

#[derive(Clone, Debug)]
pub struct DataLayout {
    pub endianness: Endianness,
    pub mangling: Option<Mangling>,
    pub stack_alignment: Option<u32>,
    pub pointer_layouts: HashMap<AddrSpace, (u32, AlignmentInfo)>,
    pub type_layouts: HashMap<(AlignType, u32), AlignmentInfo>,
    pub aggregate_layout: AlignmentInfo,
    pub native_sizes: Option<HashSet<u32>>,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum Endianness {
    LittleEndian,
    BigEndian,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum Mangling {
    ELF,
    MIPS,
    MachO,
    WindowsCOFF,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct AlignmentInfo {
    pub abi_alignment: u32,
    pub preferred_alignment: u32,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug, Hash)]
pub enum AlignType {
    Integer,
    Vector,
    Float,
}

*/

// ********* //
// from_llvm //
// ********* //

use crate::constant::GlobalNameMap;
use crate::from_llvm::*;
use crate::types::TyNameMap;
use crate::metadata::LLVMToNodeIDMap;
use llvm_sys::{LLVMDLLStorageClass, LLVMLinkage, LLVMThreadLocalMode, LLVMUnnamedAddr, LLVMVisibility};
use llvm_sys::comdat::*;

impl Module {
    pub(crate) fn from_llvm_ref(module: LLVMModuleRef) -> Self {
        debug!("Creating a Module from an LLVMModuleRef");
        let mut global_ctr = 0; // this ctr is used to number global objects that aren't named

        // Modules require two passes over their contents.
        // First we make a pass just to map global objects -- in particular, Functions,
        //   GlobalVariables, and GlobalAliases -- to Names; then we do the actual
        //   detailed pass.
        // This is necessary because these structures may reference each other in a
        //   circular fashion, and we need to be able to fill in the Name of the
        //   referenced object from having only its `LLVMValueRef`.
        let gnmap: GlobalNameMap = get_defined_functions(module)
            .chain(get_declared_functions(module))
            .chain(get_globals(module))
            .chain(get_global_aliases(module))
            .map(|g| {
                (
                    g,
                    Name::name_or_num(unsafe { get_value_name(g) }, &mut global_ctr),
                )
            })
            .collect();
        global_ctr = 0; // reset the global_ctr; the second pass should number everything exactly the same though

        let mut tynamemap = TyNameMap::new();
        let mut llmap: LLVMToNodeIDMap = LLVMToNodeIDMap::new();
        let mut mnmap: MetadataNodeMap = MetadataNodeMap::new();

        Self {
            name: unsafe { get_module_identifier(module) },
            source_file_name: unsafe { get_source_file_name(module) },
            data_layout: unsafe { get_data_layout_str(module) },
            target_triple: unsafe { get_target(module) },
            functions: get_defined_functions(module)
                .map(|f| Function::from_llvm_ref(f, &gnmap, &mut tynamemap))
                .collect(),
            global_vars: get_globals(module)
                .map(|g| GlobalVariable::from_llvm_ref(g, &mut global_ctr, &gnmap, &mut tynamemap))
                .collect(),
            global_aliases: get_global_aliases(module)
                .map(|g| GlobalAlias::from_llvm_ref(g, &mut global_ctr, &gnmap, &mut tynamemap))
                .collect(),
            // function_attribute_groups: unimplemented!("function_attribute_groups"),  // llvm-hs collects these in the decoder monad or something
            named_struct_types: tynamemap,
            inline_assembly: unsafe { get_module_inline_asm(module) },
            metadata_nodes: mnmap,
            named_metadatas: get_named_metadatas(module).map(|nm| NamedMetadata::from_llvm_ref(nm, module, &llmap)).collect(),
            // comdats: unimplemented!("comdats"),  // I think llvm-hs also collects these along the way
        }
    }
}

impl GlobalVariable {
    pub(crate) fn from_llvm_ref(
        global: LLVMValueRef,
        ctr: &mut usize,
        gnmap: &GlobalNameMap,
        tnmap: &mut TyNameMap,
    ) -> Self {
        let ty = Type::from_llvm_ref(unsafe { LLVMTypeOf(global) }, tnmap);
        debug!("Processing a GlobalVariable with type {:?}", ty);
        Self {
            name: Name::name_or_num(unsafe { get_value_name(global) }, ctr),
            linkage: Linkage::from_llvm(unsafe { LLVMGetLinkage(global) }),
            visibility: Visibility::from_llvm(unsafe { LLVMGetVisibility(global) }),
            is_constant: unsafe { LLVMIsGlobalConstant(global) } != 0,
            ty: ty.clone(),
            addr_space: match ty {
                Type::PointerType { addr_space, .. } => addr_space,
                _ => panic!("GlobalVariable has a non-pointer type, {:?}", ty),
            },
            dll_storage_class: DLLStorageClass::from_llvm(unsafe { LLVMGetDLLStorageClass(global) }),
            thread_local_mode: ThreadLocalMode::from_llvm(unsafe { LLVMGetThreadLocalMode(global) }),
            unnamed_addr: UnnamedAddr::from_llvm(unsafe { LLVMGetUnnamedAddress(global) }),
            initializer: {
                let it = unsafe { LLVMGetInitializer(global) };
                if it.is_null() {
                    None
                } else {
                    Some(Constant::from_llvm_ref(it, gnmap, tnmap))
                }
            },
            section: unsafe { get_section(global) },
            comdat: {
                let comdat = unsafe { LLVMGetComdat(global) };
                if comdat.is_null() {
                    None
                } else {
                    Some(Comdat::from_llvm_ref(unsafe { LLVMGetComdat(global) }))
                }
            },
            alignment: unsafe { LLVMGetAlignment(global) },
            // metadata: unimplemented!("metadata"),
        }
    }
}

impl GlobalAlias {
    pub(crate) fn from_llvm_ref(
        alias: LLVMValueRef,
        ctr: &mut usize,
        gnmap: &GlobalNameMap,
        tnmap: &mut TyNameMap,
    ) -> Self {
        let ty = Type::from_llvm_ref(unsafe { LLVMTypeOf(alias) }, tnmap);
        Self {
            name: Name::name_or_num(unsafe { get_value_name(alias) }, ctr),
            aliasee: Constant::from_llvm_ref(unsafe { LLVMAliasGetAliasee(alias) }, gnmap, tnmap),
            linkage: Linkage::from_llvm(unsafe { LLVMGetLinkage(alias) }),
            visibility: Visibility::from_llvm(unsafe { LLVMGetVisibility(alias) }),
            ty: ty.clone(),
            addr_space: match ty {
                Type::PointerType { addr_space, .. } => addr_space,
                _ => panic!("GlobalAlias has a non-pointer type, {:?}", ty),
            },
            dll_storage_class: DLLStorageClass::from_llvm(unsafe { LLVMGetDLLStorageClass(alias) }),
            thread_local_mode: ThreadLocalMode::from_llvm(unsafe { LLVMGetThreadLocalMode(alias) }),
            unnamed_addr: UnnamedAddr::from_llvm(unsafe { LLVMGetUnnamedAddress(alias) }),
        }
    }
}

impl NamedMetadata {
    pub(crate) fn from_llvm_ref(nm: LLVMNamedMDNodeRef, module: LLVMModuleRef, llmap: &LLVMToNodeIDMap) -> Self {
        let raw_name = unsafe {
            let mut len = 0;
            LLVMGetNamedMetadataName(nm, &mut len)
        };
        Self {
            name: unsafe { raw_to_string(raw_name) },
            node_ids: {
                let num_nodes = unsafe { LLVMGetNamedMetadataNumOperands(module, raw_name) };
                let mut nodes: Vec<LLVMValueRef> = Vec::with_capacity(num_nodes as usize);
                unsafe {
                    LLVMGetNamedMetadataOperands(module, raw_name, nodes.as_mut_ptr());
                    nodes.set_len(num_nodes as usize);
                }
                nodes.into_iter().map(|node| llmap.get(&node).expect("Failed to find metadata node in map")).copied().collect()
            }
        }
    }
}

impl UnnamedAddr {
    pub(crate) fn from_llvm(ua: LLVMUnnamedAddr) -> Option<Self> {
        match ua {
            LLVMUnnamedAddr::LLVMNoUnnamedAddr => None,
            LLVMUnnamedAddr::LLVMLocalUnnamedAddr => Some(UnnamedAddr::Local),
            LLVMUnnamedAddr::LLVMGlobalUnnamedAddr => Some(UnnamedAddr::Global),
        }
    }
}

impl Linkage {
    pub(crate) fn from_llvm(linkage: LLVMLinkage) -> Self {
        match linkage {
            LLVMLinkage::LLVMExternalLinkage => Linkage::External,
            LLVMLinkage::LLVMAvailableExternallyLinkage => Linkage::AvailableExternally,
            LLVMLinkage::LLVMLinkOnceAnyLinkage => Linkage::LinkOnceAny,
            LLVMLinkage::LLVMLinkOnceODRLinkage => Linkage::LinkOnceODR,
            LLVMLinkage::LLVMLinkOnceODRAutoHideLinkage => Linkage::LinkOnceODRAutoHide,
            LLVMLinkage::LLVMWeakAnyLinkage => Linkage::WeakAny,
            LLVMLinkage::LLVMWeakODRLinkage => Linkage::WeakODR,
            LLVMLinkage::LLVMAppendingLinkage => Linkage::Appending,
            LLVMLinkage::LLVMInternalLinkage => Linkage::Internal,
            LLVMLinkage::LLVMPrivateLinkage => Linkage::Private,
            LLVMLinkage::LLVMDLLImportLinkage => Linkage::DLLImport,
            LLVMLinkage::LLVMDLLExportLinkage => Linkage::DLLExport,
            LLVMLinkage::LLVMExternalWeakLinkage => Linkage::ExternalWeak,
            LLVMLinkage::LLVMGhostLinkage => Linkage::Ghost,
            LLVMLinkage::LLVMCommonLinkage => Linkage::Common,
            LLVMLinkage::LLVMLinkerPrivateLinkage => Linkage::LinkerPrivate,
            LLVMLinkage::LLVMLinkerPrivateWeakLinkage => Linkage::LinkerPrivateWeak,
        }
    }
}

impl Visibility {
    pub(crate) fn from_llvm(visibility: LLVMVisibility) -> Self {
        match visibility {
            LLVMVisibility::LLVMDefaultVisibility => Visibility::Default,
            LLVMVisibility::LLVMHiddenVisibility => Visibility::Hidden,
            LLVMVisibility::LLVMProtectedVisibility => Visibility::Protected,
        }
    }
}

impl DLLStorageClass {
    pub(crate) fn from_llvm(dllsc: LLVMDLLStorageClass) -> Self {
        match dllsc {
            LLVMDLLStorageClass::LLVMDefaultStorageClass => DLLStorageClass::Default,
            LLVMDLLStorageClass::LLVMDLLImportStorageClass => DLLStorageClass::Import,
            LLVMDLLStorageClass::LLVMDLLExportStorageClass => DLLStorageClass::Export,
        }
    }
}

impl ThreadLocalMode {
    pub(crate) fn from_llvm(tlm: LLVMThreadLocalMode) -> Self {
        match tlm {
            LLVMThreadLocalMode::LLVMNotThreadLocal => ThreadLocalMode::NotThreadLocal,
            LLVMThreadLocalMode::LLVMGeneralDynamicTLSModel => ThreadLocalMode::GeneralDynamic,
            LLVMThreadLocalMode::LLVMLocalDynamicTLSModel => ThreadLocalMode::LocalDynamic,
            LLVMThreadLocalMode::LLVMInitialExecTLSModel => ThreadLocalMode::InitialExec,
            LLVMThreadLocalMode::LLVMLocalExecTLSModel => ThreadLocalMode::LocalExec,
        }
    }
}

impl Comdat {
    pub(crate) fn from_llvm_ref(comdat: LLVMComdatRef) -> Self {
        Self {
            name: "error: not yet implemented: Comdat.name".to_owned(), // there appears to not be a getter for this in the LLVM C API?  I could be misunderstanding something
            selection_kind: SelectionKind::from_llvm(unsafe { LLVMGetComdatSelectionKind(comdat) }),
        }
    }
}

impl SelectionKind {
    pub(crate) fn from_llvm(sk: LLVMComdatSelectionKind) -> Self {
        match sk {
            LLVMComdatSelectionKind::LLVMAnyComdatSelectionKind => SelectionKind::Any,
            LLVMComdatSelectionKind::LLVMExactMatchComdatSelectionKind => SelectionKind::ExactMatch,
            LLVMComdatSelectionKind::LLVMLargestComdatSelectionKind => SelectionKind::Largest,
            LLVMComdatSelectionKind::LLVMNoDuplicatesComdatSelectionKind => SelectionKind::NoDuplicates,
            LLVMComdatSelectionKind::LLVMSameSizeComdatSelectionKind => SelectionKind::SameSize,
        }
    }
}
