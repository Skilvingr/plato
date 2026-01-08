#![allow(unused)]

use std::mem;

#[repr(C)]
#[derive(PartialEq, PartialOrd, Copy, Clone)]
#[allow(non_camel_case_types)]
pub enum JobStatus {
    DDJVU_JOB_NOTSTARTED, /* operation was not even started */
    DDJVU_JOB_STARTED,    /* operation is in progress */
    DDJVU_JOB_OK,         /* operation terminated successfully */
    DDJVU_JOB_FAILED,     /* operation failed because of an error */
    DDJVU_JOB_STOPPED,    /* operation was interrupted by user */
}

#[repr(C)]
#[derive(PartialEq, Copy, Clone)]
#[allow(non_camel_case_types)]
pub enum MessageTag {
    DDJVU_ERROR,
    DDJVU_INFO,
    DDJVU_NEWSTREAM,
    DDJVU_DOCINFO,
    DDJVU_PAGEINFO,
    DDJVU_RELAYOUT,
    DDJVU_REDISPLAY,
    DDJVU_CHUNK,
    DDJVU_THUMBNAIL,
    DDJVU_PROGRESS,
}

#[repr(C)]
#[allow(non_camel_case_types)]
pub enum FormatStyle {
    DDJVU_FORMAT_BGR24,     /* truecolor 24 bits in BGR order */
    DDJVU_FORMAT_RGB24,     /* truecolor 24 bits in RGB order */
    DDJVU_FORMAT_RGBMASK16, /* truecolor 16 bits with masks */
    DDJVU_FORMAT_RGBMASK32, /* truecolor 32 bits with masks */
    DDJVU_FORMAT_GREY8,     /* greylevel 8 bits */
    DDJVU_FORMAT_PALETTE8,  /* paletized 8 bits (6x6x6 color cube) */
    DDJVU_FORMAT_MSBTOLSB,  /* packed bits, msb on the left */
    DDJVU_FORMAT_LSBTOMSB,  /* packed bits, lsb on the left */
}

#[repr(C)]
#[allow(non_camel_case_types)]
pub enum RenderMode {
    DDJVU_RENDER_COLOR = 0,  /* color page or stencil */
    DDJVU_RENDER_BLACK,      /* stencil or color page */
    DDJVU_RENDER_COLORONLY,  /* color page or fail */
    DDJVU_RENDER_MASKONLY,   /* stencil or fail */
    DDJVU_RENDER_BACKGROUND, /* color background layer */
    DDJVU_RENDER_FOREGROUND, /* color foreground layer */
}

pub const MINIEXP_NIL: *mut MiniExp = 0 as *mut MiniExp;
pub const MINIEXP_DUMMY: *mut MiniExp = 2 as *mut MiniExp;

pub const CACHE_SIZE: libc::c_ulong = 32 * 1024 * 1024;

pub enum ExoContext {}
pub enum ExoDocument {}
pub enum ExoFormat {}
pub enum ExoJob {}
pub enum ExoPage {}
pub enum MiniExp {}

#[link(name = "djvulibre")]
unsafe extern "C" {
    pub fn ddjvu_context_create(name: *const libc::c_char) -> *mut ExoContext;
    pub fn ddjvu_context_release(ctx: *mut ExoContext);
    pub fn ddjvu_cache_set_size(ctx: *mut ExoContext, size: libc::c_ulong);
    pub fn ddjvu_cache_clear(ctx: *mut ExoContext);
    pub fn ddjvu_message_wait(ctx: *mut ExoContext) -> *mut Message;
    pub fn ddjvu_message_pop(ctx: *mut ExoContext);
    pub fn ddjvu_document_job(doc: *mut ExoDocument) -> *mut ExoJob;
    pub fn ddjvu_page_job(page: *mut ExoPage) -> *mut ExoJob;
    pub fn ddjvu_job_status(job: *mut ExoJob) -> JobStatus;
    pub fn ddjvu_job_release(job: *mut ExoJob);
    pub fn ddjvu_document_create_by_filename_utf8(
        ctx: *mut ExoContext,
        path: *const libc::c_char,
        cache: libc::c_int,
    ) -> *mut ExoDocument;
    pub fn ddjvu_document_get_pagenum(doc: *mut ExoDocument) -> libc::c_int;
    pub fn ddjvu_page_create_by_pageno(
        doc: *mut ExoDocument,
        page_idx: libc::c_int,
    ) -> *mut ExoPage;
    pub fn ddjvu_page_create_by_pageid(
        doc: *mut ExoDocument,
        pageid: *const libc::c_char,
    ) -> *mut ExoPage;
    pub fn ddjvu_page_get_width(page: *mut ExoPage) -> libc::c_int;
    pub fn ddjvu_page_get_height(page: *mut ExoPage) -> libc::c_int;
    pub fn ddjvu_page_get_resolution(page: *mut ExoPage) -> libc::c_int;
    pub fn ddjvu_page_get_rotation(page: *mut ExoPage) -> libc::c_uint;
    pub fn ddjvu_page_render(
        page: *mut ExoPage,
        mode: RenderMode,
        p_rect: *const DjvuRect,
        r_rect: *const DjvuRect,
        fmt: *const ExoFormat,
        row_size: libc::c_ulong,
        buf: *mut u8,
    ) -> libc::c_int;
    pub fn ddjvu_format_create(
        style: FormatStyle,
        nargs: libc::c_int,
        args: *const libc::c_uint,
    ) -> *mut ExoFormat;
    pub fn ddjvu_format_release(fmt: *mut ExoFormat);
    pub fn ddjvu_format_set_row_order(fmt: *mut ExoFormat, top_to_bottom: libc::c_int);
    pub fn ddjvu_format_set_y_direction(fmt: *mut ExoFormat, top_to_bottom: libc::c_int);
    pub fn ddjvu_document_get_pagetext(
        doc: *mut ExoDocument,
        page_idx: libc::c_int,
        max_detail: *const libc::c_char,
    ) -> *mut MiniExp;
    pub fn ddjvu_document_get_outline(doc: *mut ExoDocument) -> *mut MiniExp;
    pub fn ddjvu_document_get_anno(doc: *mut ExoDocument, compat: libc::c_int) -> *mut MiniExp;
    pub fn ddjvu_document_get_pageanno(
        doc: *mut ExoDocument,
        page_idx: libc::c_int,
    ) -> *mut MiniExp;
    pub fn ddjvu_anno_get_hyperlinks(annot: *mut MiniExp) -> *mut *mut MiniExp;
    pub fn ddjvu_anno_get_metadata_keys(annot: *mut MiniExp) -> *mut *mut MiniExp;
    pub fn ddjvu_anno_get_metadata(annot: *mut MiniExp, key: *const MiniExp)
    -> *const libc::c_char;
    pub fn ddjvu_miniexp_release(document: *mut ExoDocument, exp: *mut MiniExp);
    pub fn miniexp_symbol(s: *const libc::c_char) -> *const MiniExp;
    pub fn miniexp_length(exp: *mut MiniExp) -> libc::c_int;
    pub fn miniexp_nth(n: libc::c_int, list: *mut MiniExp) -> *mut MiniExp;
    pub fn miniexp_stringp(exp: *mut MiniExp) -> libc::c_int;
    pub fn miniexp_to_str(exp: *mut MiniExp) -> *const libc::c_char;
    pub fn miniexp_to_name(sym: *mut MiniExp) -> *const libc::c_char;
}

// OK
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct DjvuRect {
    pub x: libc::c_int,
    pub y: libc::c_int,
    pub w: libc::c_uint,
    pub h: libc::c_uint,
}
impl Default for DjvuRect {
    fn default() -> Self {
        unsafe { mem::zeroed() }
    }
}

// OK
#[repr(C)]
pub union Message {
    pub any: MessageAny,
    pub error: MessageError,
    info: MessageInfo,
    new_stream: MessageNewStream,
    doc_info: MessageDocInfo,
    page_info: MessagePageInfo,
    chunk: MessageChunk,
    relayout: MessageRelayout,
    redisplay: MessageRedisplay,
    thumbnail: MessageThumbnail,
    progress: MessageProgress,
}

// OK
#[repr(C)]
#[derive(Copy, Clone)]
pub struct MessageAny {
    pub tag: MessageTag,
    context: *mut ExoContext,
    document: *mut ExoDocument,
    page: *mut ExoPage,
    job: *mut ExoJob,
}

// OK
#[derive(Copy, Clone)]
#[repr(C)]
pub struct MessageError {
    pub any: MessageAny,
    pub message: *const libc::c_char,
    function: *const libc::c_char,
    pub filename: *const libc::c_char,
    pub lineno: libc::c_int,
}

// OK
#[derive(Copy, Clone)]
#[repr(C)]
pub struct MessageInfo {
    any: MessageAny,
    message: *const libc::c_char,
}

// OK
#[derive(Copy, Clone)]
#[repr(C)]
pub struct MessageNewStream {
    any: MessageAny,
    streamid: libc::c_int,
    name: *const libc::c_char,
    url: *const libc::c_char,
}

// OK
#[derive(Copy, Clone)]
#[repr(C)]
pub struct MessageDocInfo {
    any: MessageAny,
}

// OK
#[derive(Copy, Clone)]
#[repr(C)]
pub struct MessagePageInfo {
    any: MessageAny,
}

// OK
#[derive(Copy, Clone)]
#[repr(C)]
pub struct MessageRelayout {
    any: MessageAny,
}

// OK
#[derive(Copy, Clone)]
#[repr(C)]
pub struct MessageRedisplay {
    any: MessageAny,
}

// OK
#[derive(Copy, Clone)]
#[repr(C)]
pub struct MessageChunk {
    any: MessageAny,
    chunkid: *const libc::c_char,
}

// OK
#[derive(Copy, Clone)]
#[repr(C)]
pub struct MessageThumbnail {
    any: MessageAny,
    pagenum: libc::c_int,
}

// OK
#[derive(Copy, Clone)]
#[repr(C)]
pub struct MessageProgress {
    any: MessageAny,
    status: JobStatus,
    percent: libc::c_int,
}
