so i have quetion about our new architecture about artifactory:

1. user build artifact from other platforn example  github action and push into our artifactory then user create servide and select source is from our artifactory then click deploy. 
2. user add resource the user select source from git then user deploy with current engine after finish upload then:
   - upload to artifactory
   - copy to each project directory path for edge -> edge for static into static
   - when detect is and app and build docker push to artifactory -> pull image and run it and then deployed
3. user upload prebuild wether static or edge then extract it build and artifact -> upload to artifactory -> place to directory and deploy it


 ## Summary                                                           
  
  ### Case 1 — External push → select artifact → click deploy ❌ Not   
  built at all
  
  The OCI registry accepts pushes from GitHub Actions, but there is no 
  engine path to deploy from it.  ImageSource  only has  Registry      
  (external Docker Hub) and  Git . There's no concept of "pull artifact
  from Shipyard's own registry and deploy it" — no engine variant, no  
  service config field, no API endpoint.
  
  ### Case 2 — Git → build → push to artifactory + copy + deploy ⚠️    
  Partial
  
   Sub-case                         │ Status
  ──────────────────────────────────┼──────────────────────────────────
   Git → static → push              │ ✅ Works ( run_static_git_steps 
   static_bundle  to registry       │ lines 652-700)
   Git → Docker build → push image  │ ⚠️ Metadata-only — 
   to registry                      │ record_docker_image  writes a
                                    │ stub manifest with empty layers.
                                    │ Image is not actually pushed via
                                    │ OCI blobs — only a local daemon
                                    │ tag is recorded
   Git → edge function → push       │ ❌ Missing — no 
   edge_function  to registry       │ push_edge_function  in 
                                    │ ArtifactPusher , edge deploy
                                    │ path has zero registry calls
  
  ### Case 3 — Upload pre-built → push to registry + deploy ⚠️ Partial 
  
  The upload pipeline ( run_static_upload_steps ) correctly extracts,  
  publishes, and serves the zip — but never calls  push_static_bundle .
  The zip is deleted with no trace in the artifact store. There's also 
  no upload path for edge functions at all.
  ──────
  ## Priority Fix Order
  
  1. Easiest — Add  push_static_bundle  call in
  run_static_upload_steps  before the zip cleanup (Case 3)             
  2. Medium — Add  ArtifactPusher::push_edge_function()  and wire it   
  into the edge deploy path (Case 2)
  3. Medium — Actually  docker push  to internal registry after git    
  build, not just record a stub (Case 2)
  4. Hardest — New  ImageSource::ShipyardArtifact  variant + service   
  config + deploy-from-registry engine path + API endpoint (Case 1)    
  
  Want me to start implementing these in priority order?