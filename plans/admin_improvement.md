- at /admin/deployments the table is not responsive, the colument is not align the content, also there is no pagination in there add the pagination
- at admin/deployments/provisioning the table is not responsive, the colument is not align the content, also there is no pagination in there add the pagination
- /admin/infra the swarm nodes table is not showing ip it should show the ip address
- at admin/docker the table is not responsive when small screen make it card list and have pagination also each table is should on dedicated child route
    1. containers as default /admin/docker/ or /admin/docker/container
    2. service as /admin/docker/service
    3. volumes as /admin/docker/volumes
    4. networs as /admin/docker/networks
    5. image as /admin/docker/images
   each table should have pagination
- admin/traefik each tab is use dedicated page route
    1. Setting is /admin/traefik or /admin/traefik/settings
    2. Static File /admin/traefik/static
    3. Dynamic /admin/traefik/dynamic
    4. Log stream is removed instead add button "show log stream" in /admin/traefik/setting when click open LogViewerOverlay
- admin/registry the car is not responsive yet
- /admin/mqtt move each tab into dedicated route page 
    1. Clients at /admin/mqtt or /admin/mqtt/clients also the client is not show all client currenlty its only show shipyard-api client
    2. Subscriptions at /admin/mqtt/sibscriptions
    3. Topics at /admin/mqtt/topics
- /admin/audit the table is not resposinve also not have pagination yet add responsive and pagination into it
- /orgs is showing very simple page with list organization modify its so it looks better