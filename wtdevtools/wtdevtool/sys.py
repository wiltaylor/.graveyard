import click
import platform
import os

def get_os():
    if platform.system() == 'Linux':
        ver = platform.version().lower()

        if ver.find('nixos'):
            return 'nixos'
    return 'unknown'


@click.command()
def update():

    if get_os() == 'nixos':
        os.system('sudo ~/.dotfiles/update.sh')
    else:
        click.echo('Unknown platform. Unable to update')

@click.command()
def install():
    if get_os() == 'nixos':
        click.echo('You are in NixOS! Please add the package to your config or devshell!')
    else:
        click.echo('Unknown platform. Unable to install package')

@click.command()
def uninstall():
    if get_os() == 'nixos':
        click.echo('You are in NixOS! Please remove the package from your config or devshell!')
    else:
        click.echo('Unknown platform. Unable to install package')


@click.command()
@click.argument('query')
def search(query):
    if get_os() == 'nixos':
        os.system('nix search nixpkgs ' + query)
    else:
        click.echo('Unknown platform. Unable to search for package')

@click.command()
@click.argument('query')
def find(query):
    if get_os() == 'nixos':
        os.system('nix search nixpkgs ' + query)
    else:
        click.echo('Unknown platform. Unable to search for package')
       
@click.command()
def info():
    os.system('neofetch')


@click.group()
def cli():
    pass


cli.add_command(install)
cli.add_command(uninstall)
cli.add_command(update)
cli.add_command(search)
cli.add_command(find)
cli.add_command(info)

#if __name__ == '__main__':
#    cli()

