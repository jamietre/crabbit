# Crabbit ToDo

This is not a roadmap. This is a stream of consciousness of things that I want to implement. They have not been thought out yet. 

[ ] How to handle authentication, when we're running on a server: don't want to have to log in via ssh occasionally; can we capture auth workflow or no workaround?
[ ] How to handlde long-lived token expiration (not authenticated): notify admin?
[ ] Interaction via comments on issue - we run with dangerous non-interactive permission, but can prompt be engineered such that it will ask questions if there are situations where claude does not have high confidence in the specifications, and needs clarification? In this case it should be able to comment on the issue.
[ ] Weekly API usage limit - would like to shut down the system when we've hit a certain weekly limit value. Generally don't care about hourly
[ ] Instructions for configuring API token for gh
[ ] Language-specific guidelines as part of prompt? Or can claude figure this out theirself and just go with what's in the repo?