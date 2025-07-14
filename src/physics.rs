use std::sync::RwLock;

use rapier3d::prelude::*;

pub struct PhysicsManager {
    pipeline: RwLock<PhysicsPipeline>,
    pub gravity: RwLock<Vector<f32>>,
    integration_params: RwLock<IntegrationParameters>,
    island_manager: RwLock<IslandManager>,
    broad_phase: RwLock<BroadPhaseMultiSap>,
    narrow_phase: RwLock<NarrowPhase>,
    impulse_joint_set: RwLock<ImpulseJointSet>,
    multibody_joint_set: RwLock<MultibodyJointSet>,
    ccd_solver: RwLock<CCDSolver>,
    query_pipeline: RwLock<QueryPipeline>,
    physics_hook: (),
    event_handler: (),
    pub rigidbodies: RwLock<RigidBodySet>,
    pub colliders: RwLock<ColliderSet>,
}

impl Default for PhysicsManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PhysicsManager {
    pub fn new() -> Self {
        Self {
            pipeline: RwLock::new(PhysicsPipeline::new()),
            gravity: RwLock::new(vector![0.0, -9.8, 0.0]),
            integration_params: RwLock::new(IntegrationParameters::default()),
            island_manager: RwLock::new(IslandManager::new()),
            broad_phase: RwLock::new(DefaultBroadPhase::new()),
            narrow_phase: RwLock::new(NarrowPhase::new()),
            impulse_joint_set: RwLock::new(ImpulseJointSet::new()),
            multibody_joint_set: RwLock::new(MultibodyJointSet::new()),
            ccd_solver: RwLock::new(CCDSolver::new()),
            query_pipeline: RwLock::new(QueryPipeline::new()),
            physics_hook: (),
            event_handler: (),
            rigidbodies: RwLock::new(RigidBodySet::new()),
            colliders: RwLock::new(ColliderSet::new()),
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
