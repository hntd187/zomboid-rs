use image::{GenericImage, ImageFormat, Rgb, Rgba, SubImage};
use image::{ImageBuffer, Pixel};
use std::cell::UnsafeCell;
use std::marker::{Send, Sync};
use std::ops::{Deref, DerefMut};

#[derive(Debug, Hash, PartialEq, Eq)]
pub struct Img<P: Pixel, U: GenericImage<Pixel = P>> {
    underlying: U,
}

impl<P: Pixel, U: GenericImage<Pixel = P>> Img<P, U> {
    pub fn capacity(&self) -> usize {
        self.underlying.pixels().count() * <P as Pixel>::CHANNEL_COUNT as usize
    }

    pub fn into_buffer(self) -> U {
        self.underlying
    }
}

impl<P, Container> Img<P, ImageBuffer<P, Container>>
where
    P: Pixel,
    Container: DerefMut<Target = [P::Subpixel]>,
{
    pub fn new_from_raw(width: u32, height: u32, container: Container) -> Option<Self> {
        ImageBuffer::from_raw(width, height, container).map(|image| Self { underlying: image })
    }
}

impl<P: Pixel> Img<P, ImageBuffer<P, Vec<P::Subpixel>>> {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            underlying: ImageBuffer::new(width, height),
        }
    }
    pub fn new_from_pixel(width: u32, height: u32, pixel: P) -> Self {
        Self {
            underlying: ImageBuffer::from_pixel(width, height, pixel),
        }
    }
}

impl<P: Pixel, U: GenericImage<Pixel = P>> Deref for Img<P, U> {
    type Target = U;

    fn deref(&self) -> &Self::Target {
        &self.underlying
    }
}

impl<P: Pixel, U: GenericImage<Pixel = P>> DerefMut for Img<P, U> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.underlying
    }
}

impl<P, Container> From<ImageBuffer<P, Container>> for Img<P, ImageBuffer<P, Container>>
where
    P: Pixel,
    Container: DerefMut<Target = [P::Subpixel]>,
{
    fn from(image: ImageBuffer<P, Container>) -> Self {
        Self { underlying: image }
    }
}

pub trait FromWithFormat<Container>
where
    Container: Deref<Target = [u8]>,
{
    fn from_with_format(container: Container, format: ImageFormat) -> Self;
}

macro_rules! impl_from_with_format {
    ($px_type:ident, $channel_type:ty, $to_fn:ident) => {
        impl<Container> FromWithFormat<Container>
            for Img<
                $px_type<$channel_type>,
                ImageBuffer<$px_type<$channel_type>, Vec<$channel_type>>,
            >
        where
            Container: Deref<Target = [u8]>,
        {
            fn from_with_format(container: Container, format: ImageFormat) -> Self {
                let dyn_image = image::load_from_memory_with_format(&container, format).unwrap();
                let img = dyn_image.$to_fn();

                Self::from(img)
            }
        }
    };
}

impl_from_with_format!(Rgb, u8, into_rgb8);
impl_from_with_format!(Rgb, u16, into_rgb16);
impl_from_with_format!(Rgb, f32, into_rgb32f);

impl_from_with_format!(Rgba, u8, into_rgba8);
impl_from_with_format!(Rgba, u16, into_rgba16);
impl_from_with_format!(Rgba, f32, into_rgba32f);

#[derive(Debug)]
pub struct ImageCell<P: Pixel, U: GenericImage<Pixel = P>> {
    underlying: UnsafeCell<Img<P, U>>,
}

pub struct Handout<'a, P: Pixel, U: GenericImage<Pixel = P>> {
    ic: &'a ImageCell<P, U>,
    x: u32,
    y: u32,
}

impl<P: Pixel, U: GenericImage<Pixel = P>> ImageCell<P, U> {
    pub fn new(image: Img<P, U>) -> Self {
        Self {
            underlying: UnsafeCell::new(image),
        }
    }

    pub fn into_inner(self) -> Img<P, U> {
        self.underlying.into_inner()
    }

    #[allow(clippy::mut_from_ref)]
    pub(crate) fn get_image_mut(&self) -> &mut Img<P, U> {
        unsafe { &mut *self.underlying.get() }
    }

    pub unsafe fn request_handout(&self, x: u32, y: u32) -> Handout<'_, P, U> {
        Handout { ic: self, x, y }
    }
}

impl<P: Pixel, U: GenericImage<Pixel = P>> Deref for ImageCell<P, U> {
    type Target = Img<P, U>;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.underlying.get() }
    }
}
unsafe impl<P: Pixel, U: GenericImage<Pixel = P>> Sync for ImageCell<P, U> {}
unsafe impl<P: Pixel, U: GenericImage<Pixel = P>> Send for ImageCell<P, U> {}
impl<'a, P: Pixel, U: GenericImage<Pixel = P>> Handout<'a, P, U> {
    pub fn put_pixel(&mut self, pixel: P) {
        let image = self.ic.get_image_mut();
        image.put_pixel(self.x, self.y, pixel);
    }

    pub fn unsafe_put_pixel(&mut self, pixel: P) {
        let image = self.ic.get_image_mut();
        unsafe {
            image.unsafe_put_pixel(self.x, self.y, pixel);
        }
    }

    pub fn subimage(&mut self, x: u32, y: u32, width: u32, height: u32) -> SubImage<&mut U> {
        self.ic.get_image_mut().sub_image(x, y, width, height)
    }
}
