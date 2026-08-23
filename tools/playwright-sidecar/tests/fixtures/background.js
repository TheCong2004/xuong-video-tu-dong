globalThis.__flowordProduction = {
  bind: (profileId) => ({ ok: true, profileId, contentScriptReady: true }),
  health: (profileId) => ({ protocol: 'floword-production', protocolVersion: 1, ok: true, profileId, extensionVersion: 'fixture', status: 'READY', loggedIn: true, capabilities: ['grok.image.edit'] }),
  dispatch: (request) => ({ protocol: 'floword-production', protocolVersion: 1, ok: true, requestId: request.requestId, jobId: request.jobId, stepId: request.stepId, attemptId: request.attemptId, leaseId: request.leaseId, profileId: request.profileId })
};
