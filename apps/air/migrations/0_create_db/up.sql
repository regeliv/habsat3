create table bno055_readings (
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

create table tel0157_readings (
  id integer primary key autoincrement not null,

  timestamp double not null,

  latitude_degrees double not null,
  longitude_degrees double not null,

  course_over_ground_degrees double not null,

  speed_over_ground_meters_per_second double not null,

  altitude_meters double not null,

  satellites integer not null
);

create table bmp280_readings (
  id integer primary key autoincrement not null,

  bmp_id integer not null,

  timestamp double not null,

  temperature_degrees_celsius double not null,
  pressure_pascals double not null
);

create table memory_usage (
  id integer primary key autoincrement not null,

  timestamp double not null,

  available_mib double not null,
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

create table as7341_readings (
    id integer primary key autoincrement not null,

    timestamp double not null,

    timeout bool not null,

    nm415 integer not null,
    nm445 integer not null,
    nm480 integer not null,
    nm515 integer not null,
    nm555 integer not null,
    nm590 integer not null,
    nm630 integer not null,
    nm680 integer not null,
    nir integer not null
);
