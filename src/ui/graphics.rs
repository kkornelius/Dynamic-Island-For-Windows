//! Thread-local native graphics resources and a private, embedded Inter font.
use std::{cell::RefCell, collections::HashMap, rc::Rc};
use windows::{
    Win32::Graphics::{Direct2D::*, DirectWrite::*},
    core::*,
};

pub struct Graphics {
    pub drawing: ID2D1Factory,
    pub writing: IDWriteFactory,
    pub stroke: ID2D1StrokeStyle,
    collection: IDWriteFontCollection,
    loader: IDWriteInMemoryFontFileLoader,
    formats: RefCell<HashMap<(i32, bool, bool, bool), IDWriteTextFormat>>,
}
thread_local! {static SHARED:RefCell<Option<Rc<Graphics>>>=const{RefCell::new(None)};}
impl Graphics {
    pub fn release_thread() {
        // Release native factories while the main thread is still running, before
        // Windows/CRT thread-local teardown starts dismantling their dependencies.
        SHARED.with(|slot| drop(slot.borrow_mut().take()));
    }
    pub fn shared() -> Result<Rc<Self>> {
        SHARED.with(|slot| {
            let mut slot = slot.borrow_mut();
            if let Some(g) = slot.as_ref() {
                return Ok(g.clone());
            }
            let g = Rc::new(Self::new()?);
            *slot = Some(g.clone());
            Ok(g)
        })
    }
    fn new() -> Result<Self> {
        unsafe {
            let drawing: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let stroke = drawing.CreateStrokeStyle(
                &D2D1_STROKE_STYLE_PROPERTIES {
                    startCap: D2D1_CAP_STYLE_ROUND,
                    endCap: D2D1_CAP_STYLE_ROUND,
                    dashCap: D2D1_CAP_STYLE_ROUND,
                    lineJoin: D2D1_LINE_JOIN_ROUND,
                    ..Default::default()
                },
                None,
            )?;
            let factory: IDWriteFactory5 = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let writing: IDWriteFactory = factory.cast()?;
            let loader = factory.CreateInMemoryFontFileLoader()?;
            writing.RegisterFontFileLoader(&loader)?;
            let bytes = include_bytes!("../../assets/fonts/Inter.ttf");
            let loaded = (|| -> Result<IDWriteFontCollection> {
                let file = loader.CreateInMemoryFontFileReference(
                    &writing,
                    bytes.as_ptr().cast(),
                    bytes.len() as u32,
                    None,
                )?;
                let builder = factory.CreateFontSetBuilder()?;
                builder.AddFontFile(&file)?;
                let set = builder.CreateFontSet()?;
                let factory3: IDWriteFactory3 = factory.cast()?;
                factory3.CreateFontCollectionFromFontSet(&set)?.cast()
            })();
            let collection = match loaded {
                Ok(c) => c,
                Err(e) => {
                    let _ = writing.UnregisterFontFileLoader(&loader);
                    return Err(e);
                }
            };
            let mut family = 0;
            let mut exists = false.into();
            collection.FindFamilyName(w!("Inter"), &mut family, &mut exists)?;
            if !exists.as_bool() {
                let _ = writing.UnregisterFontFileLoader(&loader);
                return Err(Error::new(
                    HRESULT(0x80004005u32 as i32),
                    "Embedded Inter font could not be loaded",
                ));
            }
            Ok(Self {
                drawing,
                writing,
                stroke,
                collection,
                loader,
                formats: RefCell::new(HashMap::new()),
            })
        }
    }
    pub fn format(
        &self,
        size: i32,
        bold: bool,
        center: bool,
        wrap: bool,
    ) -> Result<IDWriteTextFormat> {
        unsafe {
            let key = (size.max(11), bold, center, wrap);
            if let Some(f) = self.formats.borrow().get(&key) {
                return Ok(f.clone());
            }
            let f = self.writing.CreateTextFormat(
                w!("Inter"),
                &self.collection,
                if bold {
                    DWRITE_FONT_WEIGHT_SEMI_BOLD
                } else {
                    DWRITE_FONT_WEIGHT_NORMAL
                },
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                key.0 as f32,
                w!("en-us"),
            )?;
            f.SetTextAlignment(if center {
                DWRITE_TEXT_ALIGNMENT_CENTER
            } else {
                DWRITE_TEXT_ALIGNMENT_LEADING
            })?;
            f.SetParagraphAlignment(if wrap {
                DWRITE_PARAGRAPH_ALIGNMENT_NEAR
            } else {
                DWRITE_PARAGRAPH_ALIGNMENT_CENTER
            })?;
            f.SetWordWrapping(if wrap {
                DWRITE_WORD_WRAPPING_WRAP
            } else {
                DWRITE_WORD_WRAPPING_NO_WRAP
            })?;
            let ellipsis = self.writing.CreateEllipsisTrimmingSign(&f)?;
            f.SetTrimming(
                &DWRITE_TRIMMING {
                    granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
                    ..Default::default()
                },
                &ellipsis,
            )?;
            self.formats.borrow_mut().insert(key, f.clone());
            Ok(f)
        }
    }
}
impl Drop for Graphics {
    fn drop(&mut self) {
        let trace = std::env::args().any(|a| a == "--trace-shutdown");
        if trace {
            crate::storage::Store::new().log("graphics: releasing formats");
        }
        self.formats.get_mut().clear();
        if trace {
            crate::storage::Store::new().log("graphics: unregistering font loader");
        }
        unsafe {
            let _ = self.writing.UnregisterFontFileLoader(&self.loader);
        }
        if trace {
            crate::storage::Store::new().log("graphics: loader released");
        }
    }
}
