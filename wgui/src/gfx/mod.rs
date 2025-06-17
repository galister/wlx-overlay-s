pub mod cmd;
pub mod pass;
pub mod pipeline;

use std::{marker::PhantomData, slice::Iter, sync::Arc};

use cmd::{GfxCommandBuffer, XferCommandBuffer};
use pipeline::WGfxPipeline;
use vulkano::{
	DeviceSize,
	buffer::{Buffer, BufferContents, BufferCreateInfo, BufferUsage, IndexBuffer, Subbuffer},
	command_buffer::{
		AutoCommandBufferBuilder, CommandBufferUsage,
		allocator::{StandardCommandBufferAllocator, StandardCommandBufferAllocatorCreateInfo},
	},
	descriptor_set::allocator::{
		StandardDescriptorSetAllocator, StandardDescriptorSetAllocatorCreateInfo,
	},
	device::{Device, Queue},
	format::Format,
	image::{Image, ImageCreateInfo, ImageType, ImageUsage, sampler::Filter},
	instance::Instance,
	memory::{
		MemoryPropertyFlags,
		allocator::{
			AllocationCreateInfo, GenericMemoryAllocatorCreateInfo, MemoryTypeFilter,
			StandardMemoryAllocator,
		},
	},
	pipeline::graphics::{
		color_blend::{AttachmentBlend, BlendFactor, BlendOp},
		input_assembly::PrimitiveTopology,
		vertex_input::Vertex,
	},
	shader::ShaderModule,
};

pub const BLEND_ALPHA: AttachmentBlend = AttachmentBlend {
	src_color_blend_factor: BlendFactor::SrcAlpha,
	dst_color_blend_factor: BlendFactor::OneMinusSrcAlpha,
	color_blend_op: BlendOp::Add,
	src_alpha_blend_factor: BlendFactor::One,
	dst_alpha_blend_factor: BlendFactor::One,
	alpha_blend_op: BlendOp::Max,
};

pub type Vert2Buf = Subbuffer<[Vert2Uv]>;
pub type IndexBuf = IndexBuffer;
#[repr(C)]
#[derive(BufferContents, Vertex, Copy, Clone, Debug)]
pub struct Vert2Uv {
	#[format(R32G32_SFLOAT)]
	pub in_pos: [f32; 2],
	#[format(R32G32_SFLOAT)]
	pub in_uv: [f32; 2],
}

pub enum QueueType {
	Graphics,
	Transfer,
}

#[derive(Clone)]
pub struct WGfx {
	pub instance: Arc<Instance>,
	pub device: Arc<Device>,

	pub queue_gfx: Arc<Queue>,
	pub queue_xfer: Arc<Queue>,

	pub texture_filter: Filter,

	pub surface_format: Format,

	pub memory_allocator: Arc<StandardMemoryAllocator>,
	pub command_buffer_allocator: Arc<StandardCommandBufferAllocator>,
	pub descriptor_set_allocator: Arc<StandardDescriptorSetAllocator>,
}

impl WGfx {
	pub fn new_from_raw(
		instance: Arc<Instance>,
		device: Arc<Device>,
		queue_gfx: Arc<Queue>,
		queue_xfer: Arc<Queue>,
		surface_format: Format,
	) -> Arc<Self> {
		let memory_allocator = memory_allocator(device.clone());
		let command_buffer_allocator = Arc::new(StandardCommandBufferAllocator::new(
			device.clone(),
			StandardCommandBufferAllocatorCreateInfo {
				secondary_buffer_count: 32,
				..Default::default()
			},
		));
		let descriptor_set_allocator = Arc::new(StandardDescriptorSetAllocator::new(
			device.clone(),
			StandardDescriptorSetAllocatorCreateInfo::default(),
		));

		let quality_filter = if device.enabled_extensions().img_filter_cubic {
			Filter::Cubic
		} else {
			Filter::Linear
		};

		Arc::new(Self {
			instance,
			device,
			queue_gfx,
			queue_xfer,
			surface_format,
			texture_filter: quality_filter,
			memory_allocator,
			command_buffer_allocator,
			descriptor_set_allocator,
		})
	}

	pub fn empty_buffer<T>(&self, usage: BufferUsage, capacity: u64) -> anyhow::Result<Subbuffer<[T]>>
	where
		T: BufferContents + Clone,
	{
		Ok(Buffer::new_slice(
			self.memory_allocator.clone(),
			BufferCreateInfo {
				usage,
				..Default::default()
			},
			AllocationCreateInfo {
				memory_type_filter: MemoryTypeFilter::PREFER_DEVICE
					| MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
				..Default::default()
			},
			capacity,
		)?)
	}

	pub fn new_buffer<T>(
		&self,
		usage: BufferUsage,
		contents: Iter<'_, T>,
	) -> anyhow::Result<Subbuffer<[T]>>
	where
		T: BufferContents + Clone,
	{
		Ok(Buffer::from_iter(
			self.memory_allocator.clone(),
			BufferCreateInfo {
				usage,
				..Default::default()
			},
			AllocationCreateInfo {
				memory_type_filter: MemoryTypeFilter::PREFER_DEVICE
					| MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
				..Default::default()
			},
			contents.cloned(),
		)?)
	}

	pub fn new_image(
		&self,
		width: u32,
		height: u32,
		format: Format,
		usage: ImageUsage,
	) -> anyhow::Result<Arc<Image>> {
		Ok(Image::new(
			self.memory_allocator.clone(),
			ImageCreateInfo {
				image_type: ImageType::Dim2d,
				format,
				extent: [width, height, 1],
				usage,
				..Default::default()
			},
			AllocationCreateInfo::default(),
		)?)
	}

	pub fn create_pipeline<V>(
		self: &Arc<Self>,
		vert: Arc<ShaderModule>,
		frag: Arc<ShaderModule>,
		format: Format,
		blend: Option<AttachmentBlend>,
		topology: PrimitiveTopology,
		instanced: bool,
	) -> anyhow::Result<Arc<WGfxPipeline<V>>>
	where
		V: BufferContents + Vertex,
	{
		Ok(Arc::new(WGfxPipeline::new_with_vert_input(
			self.clone(),
			vert,
			frag,
			format,
			blend,
			topology,
			instanced,
		)?))
	}

	pub fn create_pipeline_procedural(
		self: &Arc<Self>,
		vert: Arc<ShaderModule>,
		frag: Arc<ShaderModule>,
		format: Format,
		blend: Option<AttachmentBlend>,
		topology: PrimitiveTopology,
	) -> anyhow::Result<Arc<WGfxPipeline<()>>> {
		Ok(Arc::new(WGfxPipeline::new_procedural(
			self.clone(),
			vert,
			frag,
			format,
			blend,
			topology,
		)?))
	}

	pub fn create_gfx_command_buffer(
		self: &Arc<Self>,
		usage: CommandBufferUsage,
	) -> anyhow::Result<GfxCommandBuffer> {
		self.create_gfx_command_buffer_with_queue(self.queue_gfx.clone(), usage)
	}

	pub fn create_gfx_command_buffer_with_queue(
		self: &Arc<Self>,
		queue: Arc<Queue>,
		usage: CommandBufferUsage,
	) -> anyhow::Result<GfxCommandBuffer> {
		let command_buffer = AutoCommandBufferBuilder::primary(
			self.command_buffer_allocator.clone(),
			queue.queue_family_index(),
			usage,
		)?;
		Ok(GfxCommandBuffer {
			graphics: self.clone(),
			queue,
			command_buffer,
			_dummy: PhantomData,
		})
	}

	pub fn create_xfer_command_buffer(
		self: &Arc<Self>,
		usage: CommandBufferUsage,
	) -> anyhow::Result<XferCommandBuffer> {
		self.create_xfer_command_buffer_with_queue(self.queue_gfx.clone(), usage)
	}

	pub fn create_xfer_command_buffer_with_queue(
		self: &Arc<Self>,
		queue: Arc<Queue>,
		usage: CommandBufferUsage,
	) -> anyhow::Result<XferCommandBuffer> {
		let command_buffer = AutoCommandBufferBuilder::primary(
			self.command_buffer_allocator.clone(),
			queue.queue_family_index(),
			usage,
		)?;
		Ok(XferCommandBuffer {
			graphics: self.clone(),
			queue,
			command_buffer,
			_dummy: PhantomData,
		})
	}
}

fn memory_allocator(device: Arc<Device>) -> Arc<StandardMemoryAllocator> {
	let props = device.physical_device().memory_properties();

	let mut block_sizes = vec![0; props.memory_types.len()];
	let mut memory_type_bits = u32::MAX;

	for (index, memory_type) in props.memory_types.iter().enumerate() {
		const LARGE_HEAP_THRESHOLD: DeviceSize = 1024 * 1024 * 1024;

		let heap_size = props.memory_heaps[memory_type.heap_index as usize].size;

		block_sizes[index] = if heap_size >= LARGE_HEAP_THRESHOLD {
			48 * 1024 * 1024
		} else {
			24 * 1024 * 1024
		};

		if memory_type.property_flags.intersects(
			MemoryPropertyFlags::LAZILY_ALLOCATED
				| MemoryPropertyFlags::PROTECTED
				| MemoryPropertyFlags::DEVICE_COHERENT
				| MemoryPropertyFlags::RDMA_CAPABLE,
		) {
			// VUID-VkMemoryAllocateInfo-memoryTypeIndex-01872
			// VUID-vkAllocateMemory-deviceCoherentMemory-02790
			// Lazily allocated memory would just cause problems for suballocation in general.
			memory_type_bits &= !(1 << index);
		}
	}

	let create_info = GenericMemoryAllocatorCreateInfo {
		block_sizes: &block_sizes,
		memory_type_bits,
		..Default::default()
	};

	Arc::new(StandardMemoryAllocator::new(device, create_info))
}
