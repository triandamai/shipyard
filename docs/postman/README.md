# Postman collection: Shipyard integration API

Files:

- `shipyard-integration.postman_collection.json`: every endpoint of `/openapi/v1/integration` (53 requests), plus the admin calls that create an integration and its keys.
- `shipyard-local.postman_environment.json`: an environment with `baseUrl`, `integrationKey` and `revealKey`.

## Use

1. Postman → **Import** → select both files. Pick the **Shipyard (local)** environment.
2. Set `baseUrl` to your server (for example `https://workspace.example.com`, no trailing slash).
3. Get a key, either:
   - paste an existing `ship_int_…` key into `integrationKey`, or
   - run folder **0 · Admin setup**: log in as a superadmin, create the integration, then create the keys. The scripts save `adminToken`, `integrationId`, `integrationKey` and `revealKey` for you.
4. Work through folders 1 to 6 in order. Test scripts on the requests save `orgUuid`, `publishId`, `deploymentId`, `webhookId`, `secretRef`, `appUrlRef`, `ownerUrlRef` and `eventId`, so the later requests fill themselves in.

Collection variables you set by hand: `org` (your own org id), `app` (a slug), `env` (`production`, `staging`, …), `serviceName`, `secretName`.

## Notes

- **Publish** is multipart: attach your project zip to the `source` field of the request. Without a zip it re-applies the manifest to the last upload. A `202` means the build started, not that the app is live. Poll **Get publish status** until `succeeded`.
- **Reveal a secret** uses `revealKey` (a key with `secrets:reveal`). Everything else uses `integrationKey`.
- An org created in the dashboard is invisible to integration keys until a superadmin links it (**Link an existing org to the integration**).
- Optional query parameters are shipped disabled; tick them in the request's Params tab to use them.
