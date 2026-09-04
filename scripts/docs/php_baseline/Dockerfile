FROM php:8.5-cli
ADD https://github.com/mlocati/docker-php-extension-installer/releases/latest/download/install-php-extensions /usr/local/bin/
RUN chmod +x /usr/local/bin/install-php-extensions && \
    install-php-extensions bcmath bz2 calendar dba enchant exif ffi ftp gd gettext gmp intl ldap mysqli odbc opcache pcntl pdo_dblib pdo_firebird pdo_mysql pdo_odbc pdo_pgsql pgsql shmop snmp soap sockets sysvmsg sysvsem sysvshm tidy xsl zip
RUN php -m
