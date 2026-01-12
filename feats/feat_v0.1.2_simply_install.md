I don't yet know how to fully do this, but having folks compiling the whole rust toolchain is not a good idea.

I am thinking how homebrew is doing it. I believe it is to download a precompiled binary and install it from github releases.

If that's the case, I think the workflow should be:


1. Let's get a install script writte, so that we can do `curl -sSfL https://raw.githubusercontent.com/getaifs/vibefs/HEAD/install.sh | bash` to install.
2. Figure out ways to cut the release and upload it to github releases.
3. Get the github release workflow configured, or guideme how to configure it
4. Guide me how to cut releases on Mac OS as well.