use std::sync::{Arc, RwLock};

use rapier3d::prelude::*;

pub struct PhysicsManager {
    pipeline: Arc<RwLock<PhysicsPipeline>>,
    pub gravity: Arc<RwLock<Vector<f32>>>,
    integration_params: Arc<RwLock<IntegrationParameters>>,
    island_manager: Arc<RwLock<IslandManager>>,
    broad_phase: Arc<RwLock<BroadPhaseMultiSap>>,
    narrow_phase: Arc<RwLock<NarrowPhase>>,
    impulse_joint_set: Arc<RwLock<ImpulseJointSet>>,
    multibody_joint_set: Arc<RwLock<MultibodyJointSet>>,
    ccd_solver: Arc<RwLock<CCDSolver>>,
    query_pipeline: Arc<RwLock<QueryPipeline>>,
    physics_hook: (),
    event_handler: (),
    pub rigidbodies: Arc<RwLock<RigidBodySet>>,
    pub colliders: Arc<RwLock<ColliderSet>>,
}

impl Default for PhysicsManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PhysicsManager {
    pub fn new() -> Self {
        Self {
            pipeline: Arc::new(RwLock::new(PhysicsPipeline::new())),
            gravity: Arc::new(RwLock::new(vector![0.0, -9.8, 0.0])),
            integration_params: Arc::new(RwLock::new(IntegrationParameters::default())),
            island_manager: Arc::new(RwLock::new(IslandManager::new())),
            broad_phase: Arc::new(RwLock::new(DefaultBroadPhase::new())),
            narrow_phase: Arc::new(RwLock::new(NarrowPhase::new())),
            impulse_joint_set: Arc::new(RwLock::new(ImpulseJointSet::new())),
            multibody_joint_set: Arc::new(RwLock::new(MultibodyJointSet::new())),
            ccd_solver: Arc::new(RwLock::new(CCDSolver::new())),
            query_pipeline: Arc::new(RwLock::new(QueryPipeline::new())),
            physics_hook: (),
            event_handler: (),
            rigidbodies: Arc::new(RwLock::new(RigidBodySet::new())),
            colliders: Arc::new(RwLock::new(ColliderSet::new())),
        }
    }

    pub fn destroy_object(&self, rigidbody: RigidBodyHandle) {
        let mut rigidbodies = self.rigidbodies.write().unwrap();

        rigidbodies.remove(
            rigidbody, 
            &mut self.island_manager.write().unwrap(), 
            &mut self.colliders.write().unwrap(), 
            &mut self.impulse_joint_set.write().unwrap(), 
            &mut self.multibody_joint_set.write().unwrap(), 
            true
        );
    }

    pub fn insert_collider(&self, collider: Collider) -> ColliderHandle {
        let mut colliders = self.colliders.write().unwrap();

        colliders.insert(collider)
    }

    pub fn tick(&self) {
        let mut pipeline = self.pipeline.write().unwrap();
        let mut broad_phase = self.broad_phase.write().unwrap();

        pipeline.step(
            &self.gravity.read().unwrap(),
            &self.integration_params.write().unwrap(),
            &mut self.island_manager.write().unwrap(),
            &mut *broad_phase,
            &mut self.narrow_phase.write().unwrap(),
            &mut self.rigidbodies.write().unwrap(),
            &mut self.colliders.write().unwrap(),
            &mut self.impulse_joint_set.write().unwrap(),
            &mut self.multibody_joint_set.write().unwrap(),
            &mut self.ccd_solver.write().unwrap(),
            Some(&mut self.query_pipeline.write().unwrap()),
            &self.physics_hook,
            &self.event_handler,
        );
    }
}
