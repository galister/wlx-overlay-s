use std::{marker::PhantomData, ops::Range, sync::Arc};

use smallvec::smallvec;
use vulkano::{
	buffer::{BufferContents, IndexBuffer, Subbuffer},
	command_buffer::{
		AutoCommandBufferBuilder, CommandBufferInheritanceInfo, CommandBufferInheritanceRenderPassType,
		CommandBufferInheritanceRenderingInfo, CommandBufferUsage, SecondaryAutoCommandBuffer,
	},
	descriptor_set::DescriptorSet,
	pipeline::{
		Pipeline, PipelineBindPoint,
		graphics::{vertex_input::Vertex, viewport::Viewport},
	},
};

use super::pipeline::WGfxPipeline;

pub struct WGfxPass<V> {
	pub command_buffer: Arc<SecondaryAutoCommandBuffer>,
	_dummy: PhantomData<V>,
}

impl WGfxPass<()> {
	pub(super) fn new_procedural(
		pipeline: Arc<WGfxPipeline<()>>,
		dimensions: [f32; 2],
		vertices: Range<u32>,
		instances: Range<u32>,
		descriptor_sets: Vec<Arc<DescriptorSet>>,
	) -> anyhow::Result<Self> {
		let viewport = Viewport {
			offset: [0.0, 0.0],
			extent: dimensions,
			depth_range: 0.0..=1.0,
		};
		let pipeline_inner = pipeline.inner();
		let mut command_buffer = AutoCommandBufferBuilder::secondary(
			pipeline.graphics.command_buffer_allocator.clone(),
			pipeline.graphics.queue_gfx.queue_family_index(),
			CommandBufferUsage::MultipleSubmit,
			CommandBufferInheritanceInfo {
				render_pass: Some(CommandBufferInheritanceRenderPassType::BeginRendering(
					CommandBufferInheritanceRenderingInfo {
						color_attachment_formats: vec![Some(pipeline.format)],

						..Default::default()
					},
				)),
				..Default::default()
			},
		)?;

		unsafe {
			command_buffer
				.set_viewport(0, smallvec![viewport])?
				.bind_pipeline_graphics(pipeline_inner)?
				.bind_descriptor_sets(
					PipelineBindPoint::Graphics,
					pipeline.inner().layout().clone(),
					0,
					descriptor_sets,
				)?
				.draw(
					vertices.end - vertices.start,
					instances.end - instances.start,
					vertices.start,
					instances.start,
				)?
		};

		Ok(Self {
			command_buffer: command_buffer.build()?,
			_dummy: PhantomData,
		})
	}
}

impl<V> WGfxPass<V>
where
	V: BufferContents + Vertex,
{
	pub(super) fn new_indexed(
		pipeline: Arc<WGfxPipeline<V>>,
		dimensions: [f32; 2],
		vertex_buffer: Subbuffer<[V]>,
		index_buffer: IndexBuffer,
		descriptor_sets: Vec<Arc<DescriptorSet>>,
	) -> anyhow::Result<Self> {
		let viewport = Viewport {
			offset: [0.0, 0.0],
			extent: dimensions,
			depth_range: 0.0..=1.0,
		};
		let pipeline_inner = pipeline.inner();
		let mut command_buffer = AutoCommandBufferBuilder::secondary(
			pipeline.graphics.command_buffer_allocator.clone(),
			pipeline.graphics.queue_gfx.queue_family_index(),
			CommandBufferUsage::MultipleSubmit,
			CommandBufferInheritanceInfo {
				render_pass: Some(CommandBufferInheritanceRenderPassType::BeginRendering(
					CommandBufferInheritanceRenderingInfo {
						color_attachment_formats: vec![Some(pipeline.format)],

						..Default::default()
					},
				)),
				..Default::default()
			},
		)?;

		unsafe {
			command_buffer
				.set_viewport(0, smallvec![viewport])?
				.bind_pipeline_graphics(pipeline_inner)?
				.bind_descriptor_sets(
					PipelineBindPoint::Graphics,
					pipeline.inner().layout().clone(),
					0,
					descriptor_sets,
				)?
				.bind_vertex_buffers(0, vertex_buffer)?
				.bind_index_buffer(index_buffer.clone())?
				.draw_indexed(index_buffer.len() as u32, 1, 0, 0, 0)?
		};

		Ok(Self {
			command_buffer: command_buffer.build()?,
			_dummy: PhantomData,
		})
	}

	pub(super) fn new_instanced(
		pipeline: Arc<WGfxPipeline<V>>,
		dimensions: [f32; 2],
		vertex_buffer: Subbuffer<[V]>,
		vertices: Range<u32>,
		instances: Range<u32>,
		descriptor_sets: Vec<Arc<DescriptorSet>>,
	) -> anyhow::Result<Self> {
		let viewport = Viewport {
			offset: [0.0, 0.0],
			extent: dimensions,
			depth_range: 0.0..=1.0,
		};
		let pipeline_inner = pipeline.inner();
		let mut command_buffer = AutoCommandBufferBuilder::secondary(
			pipeline.graphics.command_buffer_allocator.clone(),
			pipeline.graphics.queue_gfx.queue_family_index(),
			CommandBufferUsage::MultipleSubmit,
			CommandBufferInheritanceInfo {
				render_pass: Some(CommandBufferInheritanceRenderPassType::BeginRendering(
					CommandBufferInheritanceRenderingInfo {
						color_attachment_formats: vec![Some(pipeline.format)],

						..Default::default()
					},
				)),
				..Default::default()
			},
		)?;

		unsafe {
			command_buffer
				.set_viewport(0, smallvec![viewport])?
				.bind_pipeline_graphics(pipeline_inner)?
				.bind_descriptor_sets(
					PipelineBindPoint::Graphics,
					pipeline.inner().layout().clone(),
					0,
					descriptor_sets,
				)?
				.bind_vertex_buffers(0, vertex_buffer)?
				.draw(
					vertices.end - vertices.start,
					instances.end - instances.start,
					vertices.start,
					instances.start,
				)?
		};

		Ok(Self {
			command_buffer: command_buffer.build()?,
			_dummy: PhantomData,
		})
	}
}
