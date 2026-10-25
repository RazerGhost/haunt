/*
 * Starts the window's connection to Haunt's Rust side, in a fixed order: the repositories (and their news) first, and
 * the profile's look (with the icon sets kept on this computer); then
 * what reads the shown repository: its code, its refs, its changes (by function, and as files) and its hotspots; and
 * what listens for git's progress and questions during a fetch, pull or push, the merge or rebase that has not
 * finished, and what offers to undo the last write; and, only when switched on in Settings, pull request status from the host's tool and the update check.
 */
export default defineNuxtPlugin(async () => {
  await startRepositories()
  await startIconSets()
  startAppearance()
  startCode()
  startRefs()
  startChanges()
  startLocalChanges()
  startHotspots()
  startNetwork()
  startOperation()
  startUndo()
  startBisect()
  startWeb()
  startHostPulls()
  startUpdates()
})
