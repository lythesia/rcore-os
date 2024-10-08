import os

base_address = 0x80400000
step = 0x20000
linker = 'src/linker.ld'

apps = os.listdir('src/bin')
apps.sort()
for i, app in enumerate(apps):
    app, _ = os.path.splitext(app)
    base_old = hex(base_address)
    # app-i loaded into 0x80400000 + 0x2000*i
    base_new = hex(base_address+step*i)

    os.system(f'cp {linker} {linker}.bak')
    # linker.ld for app-i
    os.system(f"sed -i 's/{base_old}/{base_new}/g' {linker}")

    # build app-i
    os.system('cargo build --bin %s --release' % app)
    print('[build.py] application %s start with address %s' % (app, base_new))

    # restore linker.ld
    os.system(f'mv {linker}.bak {linker}')