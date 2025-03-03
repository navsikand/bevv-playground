//use bevy::prelude::*;
//use bevy_panorbit_camera::{PanOrbitCamera, PanOrbitCameraPlugin};
//
//
//fn main() {
//  App::new()
//    .add_plugins(DefaultPlugins)
//    .add_plugins(PanOrbitCameraPlugin)
//    .add_systems(Startup, setup)
//    .run();
//}
//
//fn setup(
//  mut commands: Commands,
//  mut meshes: ResMut<Assets<Mesh>>,
//  mut materials: ResMut<Assets<StandardMaterial>>,
//) {
//  let mesh = meshes.add(Sphere { radius: 0.5 });
//  let material = materials.add(Color::BLACK);
//
//  for _ in 0..1_000_000 {
//    commands.spawn((
//      Mesh3d(mesh.clone()),
//      MeshMaterial3d(material.clone()),
//      Transform::from_xyz(
//        get_random_ft(0.0, 1000.0),
//        get_random_ft(0.0, 1000.0),
//        get_random_ft(0.0, 1000.0),
//      ),
//    ));
//  }
//
//commands.insert_resource(AmbientLight {
//  color: Color::WHITE,
//  brightness: 100_000.0,
//});
//  commands.spawn((
//    Transform::from_translation(Vec3::new(0.0, 1.5, 5.0)),
//    PanOrbitCamera::default(),
//  ));
//}

//! Simple benchmark to test per-entity draw overhead.
//!
//! To measure performance realistically, be sure to run this in release mode.
//! `cargo run --example many_cubes --release`
//!
//! By default, this arranges the meshes in a spherical pattern that
//! distributes the meshes evenly.
//!
//! See `cargo run --example many_cubes --release -- --help` for more options.

//! A shader that renders a mesh multiple times in one draw call.
//!
//! Bevy will automatically batch and instance your meshes assuming you use the same
//! `Handle<Material>` and `Handle<Mesh>` for all of your instances.
//!
//! This example is intended for advanced users and shows how to make a custom instancing
//! implementation using bevy's low level rendering api.
//! It's generally recommended to try the built-in instancing before going with this approach.

use bevy::{
  core_pipeline::core_3d::Transparent3d,
  ecs::{
    query::QueryItem,
    system::{lifetimeless::*, SystemParamItem},
  },
  pbr::{
    MeshPipeline, MeshPipelineKey, RenderMeshInstances, SetMeshBindGroup,
    SetMeshViewBindGroup,
  },
  prelude::*,
  render::{
    extract_component::{ExtractComponent, ExtractComponentPlugin},
    mesh::{
      allocator::MeshAllocator, MeshVertexBufferLayoutRef, RenderMesh,
      RenderMeshBufferInfo, SphereMeshBuilder,
    },
    render_asset::RenderAssets,
    render_phase::{
      AddRenderCommand, DrawFunctions, PhaseItem, PhaseItemExtraIndex,
      RenderCommand, RenderCommandResult, SetItemPipeline, TrackedRenderPass,
      ViewSortedRenderPhases,
    },
    render_resource::{
      Buffer, BufferDescriptor, BufferInitDescriptor, BufferUsages,
      PipelineCache, RenderPipelineDescriptor, ShaderType,
      SpecializedMeshPipeline, SpecializedMeshPipelineError,
      SpecializedMeshPipelines, VertexAttribute, VertexBufferLayout,
      VertexFormat, VertexStepMode,
    },
    renderer::{RenderDevice, RenderQueue},
    sync_world::MainEntity,
    view::{ExtractedView, NoFrustumCulling},
    Render, RenderApp, RenderSet,
  },
};
use bevy_panorbit_camera::{PanOrbitCamera, PanOrbitCameraPlugin};
use bytemuck::{Pod, Zeroable};
use rand::{distributions::Uniform, prelude::Distribution};

const NUM_PARTICLES: u32 = 1_000_000;

fn get_random_ft(x: f32, y: f32) -> f32 {
  let between = Uniform::from(x..y);
  let mut rng = rand::thread_rng();
  between.sample(&mut rng)
}

const SHADER_ASSET_PATH: &str = "shaders/instancing.wgsl";

fn main() {
  App::new()
    .add_plugins((DefaultPlugins, CustomMaterialPlugin, PanOrbitCameraPlugin))
    .add_systems(Startup, setup)
    .run();
}

fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
  let xy: u32 = (NUM_PARTICLES as f64).sqrt() as u32;
  // Build a single mesh to be instanced many times.
  commands.spawn((
    Mesh3d(meshes.add(SphereMeshBuilder {
      sphere: Sphere { radius: 0.1 },
      kind: bevy::render::mesh::SphereKind::Uv { sectors: 2, stacks: 2 },
    })),
    InstanceMaterialData(
      (1..=xy)
        .flat_map(|x| (1..=xy).map(move |y| (x as f32 / 10.0, y as f32 / 10.0)))
        .map(|_| InstanceData {
          position: Vec3::new(
            get_random_ft(-500.0, 500.0),
            get_random_ft(-500.0, 500.0),
            get_random_ft(-500.0, 500.0),
          ),
          scale: 1.0,
          color: Color::WHITE.to_srgba().to_f32_array(),
        })
        .collect(),
    ),
    // Consider leaving NoFrustumCulling if you plan GPU-based culling.
    // NoFrustumCulling,
  ));

  // Camera setup remains unchanged.
  commands.spawn((
    Transform::from_translation(Vec3::new(0.0, 1.5, 5.0)),
    PanOrbitCamera::default(),
  ));
}

#[derive(Component, Deref)]
struct InstanceMaterialData(Vec<InstanceData>);

impl ExtractComponent for InstanceMaterialData {
  type QueryData = &'static InstanceMaterialData;
  type QueryFilter = ();
  type Out = Self;

  fn extract_component(
    item: QueryItem<'_, Self::QueryData>,
  ) -> Option<Self::Out> {
    // In a performance‑critical scenario, you might avoid cloning by using a shared reference.
    Some(InstanceMaterialData(item.0.clone()))
  }
}

struct CustomMaterialPlugin;

impl Plugin for CustomMaterialPlugin {
  fn build(&self, app: &mut App) {
    app.add_plugins(ExtractComponentPlugin::<InstanceMaterialData>::default());
    app
      .sub_app_mut(RenderApp)
      .add_render_command::<Transparent3d, DrawCustom>()
      .init_resource::<SpecializedMeshPipelines<CustomPipeline>>()
      .add_systems(
        Render,
        (
          queue_custom.in_set(RenderSet::QueueMeshes),
          prepare_instance_buffers.in_set(RenderSet::PrepareResources),
        ),
      );
  }

  fn finish(&self, app: &mut App) {
    app.sub_app_mut(RenderApp).init_resource::<CustomPipeline>();
  }
}

#[derive(Clone, Copy, Pod, Zeroable)]
#[repr(C)]
struct InstanceData {
  position: Vec3,
  scale: f32,
  color: [f32; 4],
}

//
// Modified prepare_instance_buffers: Instead of always recreating the buffer,
// we check if an InstanceBuffer already exists and update it.
fn prepare_instance_buffers(
  mut commands: Commands,
  query: Query<(Entity, &InstanceMaterialData, Option<&InstanceBuffer>)>,
  render_device: Res<RenderDevice>,
  render_queue: Res<RenderQueue>,
) {
  for (entity, instance_data, maybe_buffer) in query.iter() {
    let new_data = bytemuck::cast_slice(instance_data.as_slice());
    if let Some(buffer_component) = maybe_buffer {
      // Use RenderQueue to update the buffer data
      render_queue.write_buffer(&buffer_component.buffer, 0, new_data);
    } else {
      let buffer =
        render_device.create_buffer_with_data(&BufferInitDescriptor {
          label: Some("instance data buffer"),
          contents: new_data,
          usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
        });
      commands
        .entity(entity)
        .insert(InstanceBuffer { buffer, length: instance_data.len() });
    }
  }
}

#[derive(Component)]
struct InstanceBuffer {
  buffer: Buffer,
  length: usize,
}

#[derive(Resource)]
struct CustomPipeline {
  shader: Handle<Shader>,
  mesh_pipeline: MeshPipeline,
}

impl FromWorld for CustomPipeline {
  fn from_world(world: &mut World) -> Self {
    let mesh_pipeline = world.resource::<MeshPipeline>();
    CustomPipeline {
      shader: world.load_asset(SHADER_ASSET_PATH),
      mesh_pipeline: mesh_pipeline.clone(),
    }
  }
}

impl SpecializedMeshPipeline for CustomPipeline {
  type Key = MeshPipelineKey;

  fn specialize(
    &self,
    key: Self::Key,
    layout: &MeshVertexBufferLayoutRef,
  ) -> Result<RenderPipelineDescriptor, SpecializedMeshPipelineError> {
    let mut descriptor = self.mesh_pipeline.specialize(key, layout)?;
    // Use the custom shader for both vertex and fragment stages.
    descriptor.vertex.shader = self.shader.clone();
    // Add an instance buffer layout.
    descriptor.vertex.buffers.push(VertexBufferLayout {
      array_stride: std::mem::size_of::<InstanceData>() as u64,
      step_mode: VertexStepMode::Instance,
      attributes: vec![
        VertexAttribute {
          format: VertexFormat::Float32x4,
          offset: 0,
          shader_location: 3,
        },
        VertexAttribute {
          format: VertexFormat::Float32x4,
          offset: VertexFormat::Float32x4.size(),
          shader_location: 4,
        },
      ],
    });
    descriptor.fragment.as_mut().unwrap().shader = self.shader.clone();
    Ok(descriptor)
  }
}

type DrawCustom = (
  SetItemPipeline,
  SetMeshViewBindGroup<0>,
  SetMeshBindGroup<1>,
  DrawMeshInstanced,
);

struct DrawMeshInstanced;

impl<P: PhaseItem> RenderCommand<P> for DrawMeshInstanced {
  type Param = (
    SRes<RenderAssets<RenderMesh>>,
    SRes<RenderMeshInstances>,
    SRes<MeshAllocator>,
  );
  type ViewQuery = ();
  type ItemQuery = Read<InstanceBuffer>;

  #[inline]
  fn render<'w>(
    item: &P,
    _view: (),
    instance_buffer: Option<&'w InstanceBuffer>,
    (meshes, render_mesh_instances, mesh_allocator): SystemParamItem<
      'w,
      '_,
      Self::Param,
    >,
    pass: &mut TrackedRenderPass<'w>,
  ) -> RenderCommandResult {
    let mesh_allocator = mesh_allocator.into_inner();

    let Some(mesh_instance) =
      render_mesh_instances.render_mesh_queue_data(item.main_entity())
    else {
      return RenderCommandResult::Skip;
    };
    let Some(gpu_mesh) = meshes.into_inner().get(mesh_instance.mesh_asset_id)
    else {
      return RenderCommandResult::Skip;
    };
    let Some(instance_buffer) = instance_buffer else {
      return RenderCommandResult::Skip;
    };
    let Some(vertex_buffer_slice) =
      mesh_allocator.mesh_vertex_slice(&mesh_instance.mesh_asset_id)
    else {
      return RenderCommandResult::Skip;
    };

    pass.set_vertex_buffer(0, vertex_buffer_slice.buffer.slice(..));
    pass.set_vertex_buffer(1, instance_buffer.buffer.slice(..));

    match &gpu_mesh.buffer_info {
      RenderMeshBufferInfo::Indexed { index_format, count } => {
        let Some(index_buffer_slice) =
          mesh_allocator.mesh_index_slice(&mesh_instance.mesh_asset_id)
        else {
          return RenderCommandResult::Skip;
        };
        pass.set_index_buffer(
          index_buffer_slice.buffer.slice(..),
          0,
          *index_format,
        );
        pass.draw_indexed(
          index_buffer_slice.range.start
            ..(index_buffer_slice.range.start + count),
          vertex_buffer_slice.range.start as i32,
          0..instance_buffer.length as u32,
        );
      }
      RenderMeshBufferInfo::NonIndexed => {
        pass.draw(vertex_buffer_slice.range, 0..instance_buffer.length as u32);
      }
    }
    RenderCommandResult::Success
  }
}

#[allow(clippy::too_many_arguments)]
fn queue_custom(
  transparent_3d_draw_functions: Res<DrawFunctions<Transparent3d>>,
  custom_pipeline: Res<CustomPipeline>,
  mut pipelines: ResMut<SpecializedMeshPipelines<CustomPipeline>>,
  pipeline_cache: Res<PipelineCache>,
  meshes: Res<RenderAssets<RenderMesh>>,
  render_mesh_instances: Res<RenderMeshInstances>,
  material_meshes: Query<(Entity, &MainEntity), With<InstanceMaterialData>>,
  mut transparent_render_phases: ResMut<ViewSortedRenderPhases<Transparent3d>>,
  views: Query<(Entity, &ExtractedView, &Msaa)>,
) {
  let draw_custom = transparent_3d_draw_functions.read().id::<DrawCustom>();

  for (view_entity, view, msaa) in &views {
    let Some(transparent_phase) =
      transparent_render_phases.get_mut(&view_entity)
    else {
      continue;
    };

    let msaa_key = MeshPipelineKey::from_msaa_samples(msaa.samples());
    let view_key = msaa_key | MeshPipelineKey::from_hdr(view.hdr);
    let rangefinder = view.rangefinder3d();
    for (entity, main_entity) in &material_meshes {
      let Some(mesh_instance) =
        render_mesh_instances.render_mesh_queue_data(*main_entity)
      else {
        continue;
      };
      let Some(mesh) = meshes.get(mesh_instance.mesh_asset_id) else {
        continue;
      };
      let key = view_key
        | MeshPipelineKey::from_primitive_topology(mesh.primitive_topology());
      let pipeline = pipelines
        .specialize(&pipeline_cache, &custom_pipeline, key, &mesh.layout)
        .unwrap();
      transparent_phase.add(Transparent3d {
        entity: (entity, *main_entity),
        pipeline,
        draw_function: draw_custom,
        distance: rangefinder.distance_translation(&mesh_instance.translation),
        batch_range: 0..1,
        extra_index: PhaseItemExtraIndex::NONE,
      });
    }
  }
}
