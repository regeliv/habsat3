create table bno_sensor_data (
    id integer primary key autoincrement not null,

    timestamp double not null,

    acc_x integer not null,
    acc_y integer not null,
    acc_z integer not null,

    mag_x integer not null,
    mag_y integer not null,
    mag_z integer not null,

    gyr_x integer not null,
    gyr_y integer not null,
    gyr_z integer not null,

    eul_heading integer not null,
    eul_roll integer not null,
    eul_pitch integer not null,

    qua_w integer not null,
    qua_x integer not null,
    qua_y integer not null,
    qua_z integer not null,

    lia_x integer not null,
    lia_y integer not null,
    lia_z integer not null,

    grv_x integer not null,
    grv_y integer not null,
    grv_z integer not null
);

create table memory_usage (
  id integer primary key autoincrement not null,

  timestamp double not null,

  free_mib double not null,
  total_mib double not null
);

create table filesystem_usage (
  id integer primary key autoincrement not null,

  timestamp double not null,

  free_mib double not null,
  total_mib double not null
);

create table cpu_temperature (
  id integer primary key autoincrement not null,

  timestamp double not null,

  degrees_celsius double not null
);
